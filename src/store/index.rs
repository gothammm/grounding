use std::sync::Arc;

use tantivy::collector::TopDocs;
use tantivy::query::{QueryParser, TermQuery};
use tantivy::schema::IndexRecordOption;
use tantivy::schema::{Schema, STORED, STRING, TEXT, Value};
use tantivy::{doc, Index, IndexWriter, TantivyDocument};

use crate::config::Config;

use super::types::{IndexDocument, Metrics, SearchResult};

#[derive(Clone)]
pub struct Store {
    index: Arc<Index>,
    schema: Arc<SchemaHandle>,
    metrics: Arc<Metrics>,
}

struct SchemaHandle {
    title: tantivy::schema::Field,
    body: tantivy::schema::Field,
    source_url: tantivy::schema::Field,
    doc_id: tantivy::schema::Field,
}

fn build_schema() -> Schema {
    let mut schema_builder = Schema::builder();
    schema_builder.add_text_field("title", TEXT | STORED);
    schema_builder.add_text_field("body", TEXT | STORED);
    schema_builder.add_text_field("source_url", STRING | STORED);
    schema_builder.add_text_field("doc_id", STRING | STORED);
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

        Ok(Self {
            index: Arc::new(index),
            schema: Arc::new(SchemaHandle { title, body, source_url, doc_id }),
            metrics: Arc::new(Metrics::default()),
        })
    }

    pub fn metrics(&self) -> &Metrics {
        &self.metrics
    }

    pub fn index_documents(&self, docs: &[IndexDocument]) -> anyhow::Result<()> {
        let mut writer: IndexWriter<TantivyDocument> = self.index.writer(50_000_000)?;
        for doc in docs {
            writer.add_document(doc!(
                self.schema.doc_id => doc.doc_id.as_str(),
                self.schema.title => doc.title.as_str(),
                self.schema.body => doc.body.as_str(),
                self.schema.source_url => doc.source_url.as_deref().unwrap_or(""),
            ))?;
        }
        writer.commit()?;
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
        writer.add_document(doc!(
            self.schema.doc_id => doc_id,
            self.schema.title => title,
            self.schema.body => body,
            self.schema.source_url => source_url.unwrap_or(""),
        ))?;
        writer.commit()?;
        self.metrics.documents_indexed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        tracing::info!("indexed document doc_id={}", doc_id);
        Ok(())
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

    pub fn search(&self, query_str: &str, top_k: usize) -> anyhow::Result<Vec<SearchResult>> {
        self.metrics.searches_performed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
        tracing::info!("search query=\"{}\" returned {} results", query_str, results.len());
        Ok(results)
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
    let mut window_start = if best_center > half {
        best_center - half
    } else {
        0
    };
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

    if result.len() > max_chars * 3 / 2 {
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
    search_start
}

fn find_sentence_end(text: &str, pos: usize) -> usize {
    let search_end = (pos + 300).min(text.len());
    if pos >= search_end {
        return search_end;
    }
    let after = &text[pos..search_end];
    for boundary in &[". ", "! ", "? ", "\n\n", "\n"] {
        if let Some(idx) = after.find(boundary) {
            let result = pos + idx + boundary.len();
            return align_to_char_boundary(text, result);
        }
    }
    align_to_char_boundary(text, search_end)
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
