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
            Ok(index) => index,
            Err(_) => {
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
                self.schema.source_url => doc.source_url.as_str(),
            ))?;
        }
        writer.commit()?;
        self.metrics.documents_indexed.fetch_add(docs.len() as u64, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    pub fn index_document(
        &self,
        doc_id: &str,
        title: &str,
        body: &str,
        source_url: &str,
    ) -> anyhow::Result<()> {
        let mut writer: IndexWriter<TantivyDocument> = self.index.writer(50_000_000)?;
        writer.add_document(doc!(
            self.schema.doc_id => doc_id,
            self.schema.title => title,
            self.schema.body => body,
            self.schema.source_url => source_url,
        ))?;
        writer.commit()?;
        self.metrics.documents_indexed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    fn doc_to_result(&self, doc_ref: TantivyDocument, score: f32) -> SearchResult {
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
            snippet: doc_ref.get_first(self.schema.body)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .chars()
                .take(500)
                .collect(),
            source_url: doc_ref.get_first(self.schema.source_url)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
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
            results.push(self.doc_to_result(doc_ref, score));
        }
        Ok(results)
    }
}
