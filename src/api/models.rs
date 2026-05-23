use serde::Deserialize;

#[derive(Deserialize)]
pub struct QueryRequest {
    pub query: String,
    #[serde(default = "default_top_k")]
    pub top_k: usize,
}

fn default_top_k() -> usize {
    5
}

#[derive(Deserialize)]
pub struct IndexRequest {
    pub doc_id: String,
    pub title: String,
    pub body: String,
    pub source_url: String,
}

#[derive(Deserialize)]
pub struct GetDocumentRequest {
    pub doc_id: String,
}

#[derive(Deserialize)]
pub struct BatchIndexRequest {
    pub documents: Vec<IndexRequest>,
}
