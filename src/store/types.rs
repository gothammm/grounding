use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Default, Debug)]
pub struct Metrics {
    pub documents_indexed: AtomicU64,
    pub searches_performed: AtomicU64,
}

impl Metrics {
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            documents_indexed: self.documents_indexed.load(Ordering::Relaxed),
            searches_performed: self.searches_performed.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, serde::Serialize)]
pub struct MetricsSnapshot {
    pub documents_indexed: u64,
    pub searches_performed: u64,
}

pub struct IndexDocument {
    pub doc_id: String,
    pub title: String,
    pub body: String,
    pub source_url: String,
}

#[derive(Debug, serde::Serialize)]
pub struct SearchResult {
    pub score: f32,
    pub doc_id: String,
    pub title: String,
    pub snippet: String,
    pub source_url: String,
}
