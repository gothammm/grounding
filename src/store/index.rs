use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tantivy::collector::TopDocs;
use tantivy::query::{QueryParser, TermQuery};
use tantivy::schema::IndexRecordOption;
use tantivy::schema::{Schema, STORED, STRING, TEXT, Value};
use tantivy::{doc, Index, IndexWriter, TantivyDocument};

use crate::config::Config;

use super::embedding::{bytes_to_embedding, embedding_to_bytes, EmbeddingStore};
use super::types::{IndexDocument, Metrics, SearchResult};

#[derive(Clone)]
pub struct Store {
    index: Arc<Index>,
    schema: Arc<SchemaHandle>,
    metrics: Arc<Metrics>,
    embedding_store: Arc<std::sync::Mutex<EmbeddingStore>>,
}

struct SchemaHandle {
    title: tantivy::schema::Field,
    body: tantivy::schema::Field,
    source_url: tantivy::schema::Field,
    doc_id: tantivy::schema::Field,
    body_embedding: Option<(tantivy::schema::Field, bool)>,
}

fn build_schema() -> Schema {
    let mut schema_builder = Schema::builder();
    schema_builder.add_text_field("title", TEXT | STORED);
    schema_builder.add_text_field("body", TEXT | STORED);
    schema_builder.add_text_field("source_url", STRING | STORED);
    schema_builder.add_text_field("doc_id", STRING | STORED);
    schema_builder.add_bytes_field("body_embedding", STORED);
    schema_builder.build()
}

impl Store {
    pub fn open(config: &Config) -> anyhow::Result<Self> {
        std::fs::create_dir_all(&config.data_dir)?;
        std::fs::create_dir_all(&config.index_dir)?;

        let index = match Index::open_in_dir(&config.index_dir) {
            Ok(index) => {
                tracing::debug!("opened existing index at {:?}", config.index_dir);
                index
            }
            Err(_) => {
                tracing::info!("creating new index at {:?}", config.index_dir);
                let schema = build_schema();
                Index::create_in_dir(&config.index_dir, schema)?
            }
        };

        let schema = index.schema();
        let title = schema.get_field("title").expect("title field should exist");
        let body = schema.get_field("body").expect("body field should exist");
        let source_url = schema.get_field("source_url").expect("source_url field should exist");
        let doc_id = schema.get_field("doc_id").expect("doc_id field should exist");
        let body_embedding = schema.get_field("body_embedding").ok().map(|f| {
            tracing::debug!("found body_embedding field in schema");
            (f, true)
        });

        if body_embedding.is_none() {
            tracing::warn!(
                "body_embedding field not found in existing schema. \
                 Vector/hybrid search unavailable. Re-index documents to enable it."
            );
        }

        let store = Self {
            index: Arc::new(index),
            schema: Arc::new(SchemaHandle { title, body, source_url, doc_id, body_embedding }),
            metrics: Arc::new(Metrics::default()),
            embedding_store: Arc::new(std::sync::Mutex::new(EmbeddingStore::new())),
        };

        store.load_embeddings_from_index()?;

        Ok(store)
    }

    fn load_embeddings_from_index(&self) -> anyhow::Result<()> {
        let embedding_field = match self.schema.body_embedding {
            Some((f, _)) => f,
            None => return Ok(()),
        };

        let reader = self.index.reader()?;
        let searcher = reader.searcher();
        let mut store = self.embedding_store.lock().unwrap();

        if !store.is_available() {
            tracing::debug!("embedding model not available, skipping load");
            return Ok(());
        }

        let mut total = 0usize;
        for (segment_ord, segment_reader) in searcher.segment_readers().iter().enumerate() {
            let num_docs = segment_reader.num_docs() as usize;
            for doc_id in 0..num_docs {
                let doc_addr = tantivy::DocAddress::new(segment_ord as u32, doc_id as u32);
                let doc = match searcher.doc::<TantivyDocument>(doc_addr) {
                    Ok(d) => d,
                    Err(_) => continue,
                };
                if let Some(doc_id_val) = doc.get_first(self.schema.doc_id) {
                    if let Some(doc_id_str) = doc_id_val.as_str() {
                        if let Some(emb_bytes) = doc.get_first(embedding_field) {
                            if let Some(bytes) = emb_bytes.as_bytes() {
                                if !bytes.is_empty() {
                                    if let Ok(embedding) = bytes_to_embedding(bytes) {
                                        store.add(doc_id_str, embedding);
                                        total += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if total > 0 {
            tracing::info!("loaded {} embeddings from index", total);
        }

        Ok(())
    }

    pub fn metrics(&self) -> &Metrics {
        &self.metrics
    }

    pub fn index_documents(&self, docs: &[IndexDocument]) -> anyhow::Result<()> {
        let mut writer: IndexWriter<TantivyDocument> = self.index.writer(50_000_000)?;

        let body_texts: Vec<&str> = docs.iter().map(|d| d.body.as_str()).collect();
        let embeddings = self.try_embed_batch(&body_texts);

        for (i, doc) in docs.iter().enumerate() {
            let mut tantivy_doc = doc!(
                self.schema.doc_id => doc.doc_id.as_str(),
                self.schema.title => doc.title.as_str(),
                self.schema.body => doc.body.as_str(),
                self.schema.source_url => doc.source_url.as_deref().unwrap_or(""),
            );

            if let Some((field, _)) = self.schema.body_embedding {
                if let Some(ref emb_vecs) = embeddings {
                    let bytes = embedding_to_bytes(&emb_vecs[i]);
                    tantivy_doc.add_bytes(field, &bytes);
                } else {
                    tantivy_doc.add_bytes(field, &[]);
                }
            }

            writer.add_document(tantivy_doc)?;
        }

        writer.commit()?;

        if let Some(ref emb_vecs) = embeddings {
            let mut store = self.embedding_store.lock().unwrap();
            for (i, doc) in docs.iter().enumerate() {
                store.add(&doc.doc_id, emb_vecs[i].clone());
            }
        }

        let count = docs.len() as u64;
        self.metrics.documents_indexed.fetch_add(count, std::sync::atomic::Ordering::Relaxed);
        tracing::info!("indexed {} documents", count);
        Ok(())
    }

    pub fn index_document(
        &self,
        doc_id: &str,
        title: &str,
        body: &str,
        source_url: Option<&str>,
    ) -> anyhow::Result<()> {
        let mut writer: IndexWriter<TantivyDocument> = self.index.writer(50_000_000)?;

        let mut tantivy_doc = doc!(
            self.schema.doc_id => doc_id,
            self.schema.title => title,
            self.schema.body => body,
            self.schema.source_url => source_url.unwrap_or(""),
        );

        let embedding = self.try_embed(body);
        if let Some((field, _)) = self.schema.body_embedding {
            if let Some(ref emb) = embedding {
                let bytes = embedding_to_bytes(emb);
                tantivy_doc.add_bytes(field, &bytes);
            } else {
                tantivy_doc.add_bytes(field, &[]);
            }
        }

        writer.add_document(tantivy_doc)?;
        writer.commit()?;

        if let Some(emb) = embedding {
            let mut store = self.embedding_store.lock().unwrap();
            store.add(doc_id, emb);
        }

        self.metrics.documents_indexed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        tracing::info!("indexed document doc_id={}", doc_id);
        Ok(())
    }

    fn try_embed(&self, text: &str) -> Option<Vec<f32>> {
        self.schema.body_embedding?;
        let store = self.embedding_store.lock().unwrap();
        if !store.is_available() { return None; }
        store.embed_batch(&[text]).ok().map(|v| v.into_iter().next().unwrap())
    }

    fn try_embed_batch(&self, texts: &[&str]) -> Option<Vec<Vec<f32>>> {
        self.schema.body_embedding?;
        let store = self.embedding_store.lock().unwrap();
        if !store.is_available() { return None; }
        store.embed_batch(texts).ok()
    }

    pub fn search(&self, query_str: &str, top_k: usize) -> anyhow::Result<Vec<SearchResult>> {
        self.search_with_mode(query_str, top_k, "hybrid")
    }

    pub fn search_with_mode(
        &self,
        query_str: &str,
        top_k: usize,
        mode: &str,
    ) -> anyhow::Result<Vec<SearchResult>> {
        self.metrics.searches_performed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        match mode {
            "bm25" => self.bm25_search(query_str, top_k),
            "vector" => self.vector_search(query_str, top_k),
            _ => self.hybrid_search(query_str, top_k),
        }
    }

    fn bm25_search(&self, query_str: &str, top_k: usize) -> anyhow::Result<Vec<SearchResult>> {
        let reader = self.index.reader()?;
        let searcher = reader.searcher();
        let query_parser = QueryParser::for_index(&self.index, vec![
            self.schema.title,
            self.schema.body,
        ]);
        let query = query_parser.parse_query(query_str)?;
        let top_docs = searcher.search(&query, &TopDocs::with_limit(top_k).order_by_score())?;

        let mut results = Vec::new();
        for (score, doc_addr) in top_docs {
            let doc_ref = searcher.doc::<TantivyDocument>(doc_addr)?;
            results.push(self.doc_to_search_result(doc_ref, score, query_str));
        }
        tracing::debug!("bm25 search query=\"{}\" returned {} results", query_str, results.len());
        Ok(results)
    }

    fn vector_search(&self, query_str: &str, top_k: usize) -> anyhow::Result<Vec<SearchResult>> {
        let store = self.embedding_store.lock().unwrap();
        if !store.is_available() || store.is_empty() {
            return Ok(Vec::new());
        }
        let query_vec = match store.embed_batch(&[query_str]) {
            Ok(v) => v.into_iter().next().unwrap(),
            Err(_) => return Ok(Vec::new()),
        };
        let vec_results = store.search(&query_vec, top_k);
        drop(store);

        let mut results = Vec::new();
        for (doc_id, score) in vec_results {
            if let Ok(Some(mut doc)) = self.get_document(&doc_id) {
                doc.score = score;
                results.push(doc);
            }
        }
        tracing::debug!("vector search query=\"{}\" returned {} results", query_str, results.len());
        Ok(results)
    }

    fn hybrid_search(&self, query_str: &str, top_k: usize) -> anyhow::Result<Vec<SearchResult>> {
        let expanded_k = top_k * 3;
        let rrf_k = 60.0;

        let bm25_results = self.bm25_search(query_str, expanded_k)?;

        let (vec_results, embedder_available) = {
            let store = self.embedding_store.lock().unwrap();
            if !store.is_available() || store.is_empty() {
                (Vec::new(), false)
            } else {
                let query_vec = match store.embed_batch(&[query_str]) {
                    Ok(v) => v.into_iter().next().unwrap(),
                    Err(_) => return Ok(bm25_results),
                };
                (store.search(&query_vec, expanded_k), true)
            }
        };

        if !embedder_available {
            return Ok(bm25_results);
        }

        let mut bm25_ranks: HashMap<&str, usize> = HashMap::new();
        for (i, r) in bm25_results.iter().enumerate() {
            bm25_ranks.insert(r.doc_id.as_str(), i + 1);
        }

        let mut vec_ranks: HashMap<&str, usize> = HashMap::new();
        for (i, (id, _)) in vec_results.iter().enumerate() {
            vec_ranks.insert(id.as_str(), i + 1);
        }

        let all_ids: HashSet<&str> = bm25_ranks.keys().copied().collect();

        let mut fused: Vec<(&str, f32)> = all_ids
            .into_iter()
            .map(|id| {
                let b_rank = bm25_ranks.get(id).copied().unwrap_or(expanded_k + 1);
                let v_rank = vec_ranks.get(id).copied().unwrap_or(expanded_k + 1);
                let score = 1.0 / (rrf_k + b_rank as f32) + 1.0 / (rrf_k + v_rank as f32);
                (id, score)
            })
            .collect();

        fused.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        fused.truncate(top_k);

        let mut bm25_map: HashMap<&str, &SearchResult> = HashMap::new();
        for r in &bm25_results {
            bm25_map.insert(r.doc_id.as_str(), r);
        }

        let mut results = Vec::new();
        for (id, score) in fused {
            if let Some(bm25_result) = bm25_map.get(id) {
                let result = SearchResult {
                    score,
                    doc_id: bm25_result.doc_id.clone(),
                    title: bm25_result.title.clone(),
                    snippet: bm25_result.snippet.clone(),
                    source_url: bm25_result.source_url.clone(),
                };
                results.push(result);
            } else if let Ok(Some(mut result)) = self.get_document(id) {
                result.score = score;
                results.push(result);
            }
        }
        tracing::debug!("hybrid search query=\"{}\" returned {} results", query_str, results.len());
        Ok(results)
    }

    fn doc_to_result(&self, doc_ref: TantivyDocument, score: f32) -> SearchResult {
        fn empty_to_none(s: &str) -> Option<String> {
            if s.is_empty() { None } else { Some(s.to_string()) }
        }
        let body = doc_ref.get_first(self.schema.body)
            .and_then(|v| v.as_str())
            .unwrap_or("");
        SearchResult {
            score,
            doc_id: doc_ref.get_first(self.schema.doc_id)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            title: doc_ref.get_first(self.schema.title)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            snippet: body.chars().take(500).collect(),
            source_url: doc_ref.get_first(self.schema.source_url)
                .and_then(|v| v.as_str())
                .and_then(empty_to_none),
        }
    }

    fn doc_to_search_result(&self, doc_ref: TantivyDocument, score: f32, query: &str) -> SearchResult {
        fn empty_to_none(s: &str) -> Option<String> {
            if s.is_empty() { None } else { Some(s.to_string()) }
        }
        let body = doc_ref.get_first(self.schema.body)
            .and_then(|v| v.as_str())
            .unwrap_or("");
        SearchResult {
            score,
            doc_id: doc_ref.get_first(self.schema.doc_id)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            title: doc_ref.get_first(self.schema.title)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            snippet: extract_relevant_passage(body, query, 500),
            source_url: doc_ref.get_first(self.schema.source_url)
                .and_then(|v| v.as_str())
                .and_then(empty_to_none),
        }
    }

    pub fn get_document(&self, doc_id: &str) -> anyhow::Result<Option<SearchResult>> {
        let reader = self.index.reader()?;
        let searcher = reader.searcher();
        let term = tantivy::Term::from_field_text(self.schema.doc_id, doc_id);
        let query = TermQuery::new(term, IndexRecordOption::Basic);
        let top_docs = searcher.search(&query, &TopDocs::with_limit(1).order_by_score())?;

        let result = top_docs.into_iter().next().and_then(|(_score, doc_addr)| {
            let doc_ref = searcher.doc::<TantivyDocument>(doc_addr).ok()?;
            Some(self.doc_to_result(doc_ref, 0.0))
        });

        if result.is_some() {
            tracing::info!("found document doc_id={}", doc_id);
        } else {
            tracing::warn!("document not found doc_id={}", doc_id);
        }

        Ok(result)
    }
}

/// Extract a relevant passage from a document body centered around query term matches.
/// Returns a context window containing the densest cluster of query term hits,
/// snapped to sentence boundaries when possible.
fn extract_relevant_passage(body: &str, query: &str, max_chars: usize) -> String {
    if body.len() <= max_chars {
        return body.to_string();
    }

    let query_terms: Vec<String> = query
        .split_whitespace()
        .filter(|t| !t.is_empty())
        .map(|t| t.to_lowercase())
        .collect();

    if query_terms.is_empty() {
        return body.chars().take(max_chars).collect();
    }

    let body_lower = body.to_lowercase();

    let mut positions: Vec<usize> = Vec::new();
    for term in &query_terms {
        let mut start = 0;
        while let Some(pos) = body_lower[start..].find(term.as_str()) {
            positions.push(start + pos);
            start = start + pos + 1;
        }
    }

    if positions.is_empty() {
        return body.chars().take(max_chars).collect();
    }

    positions.sort();
    positions.dedup();

    let best_center = if positions.len() == 1 {
        positions[0]
    } else {
        let mut best_count = 0usize;
        let mut best_center = 0usize;
        let mut left = 0;

        for right in 0..positions.len() {
            while positions[right] - positions[left] > max_chars {
                left += 1;
            }
            let count = right - left + 1;
            if count > best_count {
                best_count = count;
                best_center = (positions[left] + positions[right]) / 2;
            }
        }
        best_center
    };

    let half = max_chars / 2;
    let mut window_start = best_center.saturating_sub(half);
    window_start = align_to_char_boundary(body, window_start);
    let window_start = window_start.min(body.len().saturating_sub(max_chars));
    let window_end = (window_start + max_chars).min(body.len());
    let window_end = align_to_char_boundary(body, window_end);

    let snap_start = find_sentence_start(body, window_start);
    let snap_end = find_sentence_end(body, window_end);

    let mut result = String::new();
    if snap_start > 0 {
        result.push_str("...");
    }
    result.push_str(&body[snap_start..snap_end]);
    if snap_end < body.len() {
        result.push_str("...");
    }

    if result.len() > max_chars.saturating_mul(3) / 2 {
        result = String::new();
        if window_start > 0 {
            result.push_str("...");
        }
        result.push_str(&body[window_start..window_end]);
        if window_end < body.len() {
            result.push_str("...");
        }
    }

    result
}

fn find_sentence_start(text: &str, pos: usize) -> usize {
    let search_start = pos.saturating_sub(300);
    let before = &text[search_start..pos];
    for boundary in &["\n\n", ". ", "! ", "? ", "\n"] {
        if let Some(idx) = before.rfind(boundary) {
            let result = search_start + idx + boundary.len();
            return align_to_char_boundary(text, result);
        }
    }
    align_to_char_boundary(text, pos)
}

fn find_sentence_end(text: &str, pos: usize) -> usize {
    let search_end = (pos + 300).min(text.len());
    if pos >= search_end {
        return align_to_char_boundary(text, pos);
    }
    let after = &text[pos..search_end];
    for boundary in &[". ", "! ", "? ", "\n\n", "\n"] {
        if let Some(idx) = after.find(boundary) {
            let result = pos + idx + boundary.len();
            return align_to_char_boundary(text, result);
        }
    }
    align_to_char_boundary(text, pos)
}

fn align_to_char_boundary(text: &str, byte_pos: usize) -> usize {
    if byte_pos >= text.len() {
        return text.len();
    }
    let mut pos = byte_pos;
    while pos > 0 && !text.is_char_boundary(pos) {
        pos -= 1;
    }
    pos
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use tempfile::TempDir;

    fn setup_store() -> (Store, TempDir) {
        let tmp = TempDir::new().unwrap();
        let config = Config::new(tmp.path());
        let store = Store::open(&config).unwrap();
        (store, tmp)
    }

    #[test]
    fn test_index_and_search_document() {
        let (store, _tmp) = setup_store();
        store.index_document("doc1", "Test Document",
            "This is a test document about searching and indexing",
            Some("https://example.com/doc1")).unwrap();
        let results = store.search("searching", 5).unwrap();
        assert!(!results.is_empty());
        assert!(results[0].title.contains("Test Document"));
    }

    #[test]
    fn test_search_returns_score_ordered() {
        let (store, _tmp) = setup_store();
        store.index_document("doc1", "Rust",
            "Rust is a systems programming language",
            Some("https://example.com/rust")).unwrap();
        store.index_document("doc2", "Python",
            "Python is a scripting language",
            Some("https://example.com/python")).unwrap();
        let results = store.search("language", 5).unwrap();
        assert!(results.len() >= 2);
        for i in 1..results.len() {
            assert!(results[i-1].score >= results[i].score);
        }
    }

    #[test]
    fn test_snippet_contains_relevant_content() {
        let (store, _tmp) = setup_store();
        let long_body = "word ".repeat(200);
        store.index_document("doc1", "Test", &long_body,
            Some("https://example.com")).unwrap();
        let results = store.search("word", 5).unwrap();
        assert!(!results.is_empty());
        assert!(results[0].snippet.contains("word"));
        assert!(results[0].snippet.len() < long_body.len());
    }

    #[test]
    fn test_passage_extraction_with_unicode() {
        let (store, _tmp) = setup_store();
        let body = "The café menu features crème brûlée and café au lait. \
            The crème brûlée is caramelized perfectly. \
            Many customers order the café au lait with dessert.";
        store.index_document("doc1", "Café Menu", body,
            Some("https://example.com/cafe")).unwrap();
        let results = store.search("café", 5).unwrap();
        assert!(!results.is_empty());
        let snippet = &results[0].snippet;
        assert!(snippet.contains("caf"), "Unicode passage should contain term");
        assert!(snippet.len() > 20);
        assert!(!snippet.contains("�"), "Snippet should not contain broken Unicode");
    }

    #[test]
    fn test_passage_extraction_targets_query_terms() {
        let (store, _tmp) = setup_store();
        let body = "The quick brown fox jumps over the lazy dog. \
            The fox is quick and agile. The dog is slow and lazy.";
        store.index_document("doc1", "Fox Story", body,
            Some("https://example.com/fox")).unwrap();
        store.index_document("doc2", "Dog Story",
            "The lazy dog sleeps all day. The dog is very lazy indeed. Nothing about foxes here.",
            Some("https://example.com/dog")).unwrap();
        let results = store.search("fox", 5).unwrap();
        assert!(!results.is_empty());
        let fox_result = results.iter().find(|r| r.doc_id == "doc1").unwrap();
        assert!(fox_result.snippet.contains("fox"),
            "Snippet should contain the query term 'fox'");
    }

    #[test]
    fn test_batch_indexing() {
        let (store, _tmp) = setup_store();
        store.index_documents(&[
            IndexDocument {
                doc_id: "a".into(), title: "Alpha".into(),
                body: "First letter of the Greek alphabet".into(),
                source_url: Some("https://example.com/alpha".into()),
            },
            IndexDocument {
                doc_id: "b".into(), title: "Beta".into(),
                body: "Second letter of the Greek alphabet".into(),
                source_url: Some("https://example.com/beta".into()),
            },
        ]).unwrap();
        let results = store.search("Greek alphabet", 5).unwrap();
        assert!(results.len() >= 2);
        let alpha = store.get_document("a").unwrap().unwrap();
        assert_eq!(alpha.title, "Alpha");
    }

    #[test]
    fn test_get_document_not_found() {
        let (store, _tmp) = setup_store();
        let result = store.get_document("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_metrics_tracking() {
        let (store, _tmp) = setup_store();
        store.index_document("m1", "X", "some content",
            Some("https://x.com")).unwrap();
        store.search("content", 5).unwrap();
        let snap = store.metrics().snapshot();
        assert_eq!(snap.documents_indexed, 1);
        assert_eq!(snap.searches_performed, 1);
    }

    #[test]
    fn test_bm25_search_mode() {
        let (store, _tmp) = setup_store();
        store.index_document("d1", "Rust Language",
            "Rust is a systems programming language focused on safety",
            None).unwrap();
        let results = store.search_with_mode("rust", 5, "bm25").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].doc_id, "d1");
    }

    #[test]
    fn test_vector_search_empty_index() {
        let (store, _tmp) = setup_store();
        let results = store.search_with_mode("something", 5, "vector").unwrap();
        assert!(results.is_empty(), "vector search on empty index should return empty");
    }

    #[test]
    fn test_vector_search_after_index() {
        let (store, _tmp) = setup_store();
        store.index_document("d1", "Programming",
            "Rust is a systems programming language focused on safety and performance",
            None).unwrap();
        let results = store.search_with_mode("rust language", 5, "vector").unwrap();

        let embedder_available = {
            let s = store.embedding_store.lock().unwrap();
            s.is_available()
        };

        if embedder_available {
            assert!(!results.is_empty(), "vector search should find results");
            assert_eq!(results[0].doc_id, "d1");
        } else {
            assert!(results.is_empty(),
                "vector search should return empty when embedder unavailable");
        }
    }

    #[test]
    fn test_hybrid_search_falls_back_to_bm25() {
        let (store, _tmp) = setup_store();
        store.index_document("d1", "Rust Language",
            "Rust is a systems programming language", None).unwrap();
        let results = store.search_with_mode("rust", 5, "hybrid").unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].doc_id, "d1");
    }

    #[test]
    fn test_default_search_is_hybrid() {
        let (store, _tmp) = setup_store();
        store.index_document("d1", "Search Test",
            "This is test content for searching", None).unwrap();
        let results = store.search("test", 5).unwrap();
        assert!(!results.is_empty());
    }

    #[test]
    fn test_search_invalid_mode_falls_back_to_hybrid() {
        let (store, _tmp) = setup_store();
        store.index_document("d1", "Test", "test content", None).unwrap();
        let results = store.search_with_mode("test", 5, "invalid").unwrap();
        assert!(!results.is_empty());
    }
}
