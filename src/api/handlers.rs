use std::sync::Arc;

use axum::{extract::State, response::IntoResponse, Json};
use serde_json::json;

use crate::store::{IndexDocument, MetricsSnapshot, Store};

use super::models::{BatchIndexRequest, GetDocumentRequest, IndexRequest, QueryRequest};

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<Store>,
}

pub async fn query_handler(
    State(state): State<AppState>,
    body: String,
) -> impl IntoResponse {
    let req: QueryRequest = match serde_json::from_str(&body) {
        Ok(r) => r,
        Err(e) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(json!({ "status": "error", "message": format!("Invalid JSON: {}", e) })),
            ).into_response()
        }
    };

    match state.store.search(&req.query, req.top_k) {
        Ok(results) => Json(json!({ "results": results })).into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "status": "error", "message": e.to_string() })),
        ).into_response(),
    }
}

pub async fn index_handler(
    State(state): State<AppState>,
    body: String,
) -> impl IntoResponse {
    let req: IndexRequest = match serde_json::from_str(&body) {
        Ok(r) => r,
        Err(e) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(json!({ "status": "error", "message": format!("Invalid JSON: {}", e) })),
            ).into_response()
        }
    };

    match state.store.index_document(&req.doc_id, &req.title, &req.body, &req.source_url) {
        Ok(()) => Json(json!({ "status": "ok" })).into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "status": "error", "message": e.to_string() })),
        ).into_response(),
    }
}

pub async fn batch_index_handler(
    State(state): State<AppState>,
    body: String,
) -> impl IntoResponse {
    let req: BatchIndexRequest = match serde_json::from_str(&body) {
        Ok(r) => r,
        Err(e) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(json!({ "status": "error", "message": format!("Invalid JSON: {}", e) })),
            ).into_response()
        }
    };

    let docs: Vec<IndexDocument> = req.documents.into_iter().map(|d| {
        IndexDocument {
            doc_id: d.doc_id,
            title: d.title,
            body: d.body,
            source_url: d.source_url,
        }
    }).collect();

    match state.store.index_documents(&docs) {
        Ok(()) => Json(json!({ "status": "ok", "count": docs.len() })).into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "status": "error", "message": e.to_string() })),
        ).into_response(),
    }
}

pub async fn get_document_handler(
    State(state): State<AppState>,
    body: String,
) -> impl IntoResponse {
    let req: GetDocumentRequest = match serde_json::from_str(&body) {
        Ok(r) => r,
        Err(e) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(json!({ "status": "error", "message": format!("Invalid JSON: {}", e) })),
            ).into_response()
        }
    };

    match state.store.get_document(&req.doc_id) {
        Ok(Some(doc)) => Json(json!(doc)).into_response(),
        Ok(None) => (
            axum::http::StatusCode::NOT_FOUND,
            Json(json!({ "status": "error", "message": "document not found" })),
        ).into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "status": "error", "message": e.to_string() })),
        ).into_response(),
    }
}

pub async fn health(State(state): State<AppState>) -> Json<serde_json::Value> {
    let store_ok = state.store.search("", 1).is_ok();
    Json(json!({
        "status": if store_ok { "ok" } else { "degraded" },
        "store": if store_ok { "healthy" } else { "unhealthy" },
    }))
}

pub async fn metrics_handler(
    State(state): State<AppState>,
) -> Json<MetricsSnapshot> {
    Json(state.store.metrics().snapshot())
}
