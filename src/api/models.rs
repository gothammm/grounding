use serde::Deserialize;

#[derive(Deserialize)]
pub struct QueryRequest {
    pub query: String,
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    #[serde(default = "default_search_mode")]
    pub mode: String,
}

fn default_top_k() -> usize {
    5
}

fn default_search_mode() -> String {
    "hybrid".to_string()
}

#[derive(Deserialize)]
pub struct IndexRequest {
    pub doc_id: String,
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub source_url: Option<String>,
}

#[derive(Deserialize)]
pub struct GetDocumentRequest {
    pub doc_id: String,
}

#[derive(Deserialize)]
pub struct BatchIndexRequest {
    pub documents: Vec<IndexRequest>,
}
