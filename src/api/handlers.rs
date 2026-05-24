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
    let req: QueryRequest = match serde_json::from_str::<QueryRequest>(&body) {
        Ok(r) => {
            tracing::debug!("query request: query=\"{}\", top_k={}", r.query, r.top_k);
            r
        }
        Err(e) => {
            tracing::warn!("query parse error: {}", e);
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(json!({ "status": "error", "message": format!("Invalid JSON: {}", e) })),
            ).into_response()
        }
    };

    match state.store.search(&req.query, req.top_k) {
        Ok(results) => {
            tracing::info!("query \"{}\" returned {} results", req.query, results.len());
            Json(json!({ "results": results })).into_response()
        }
        Err(e) => {
            tracing::error!("query \"{}\" failed: {}", req.query, e);
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "status": "error", "message": e.to_string() })),
            ).into_response()
        }
    }
}

pub async fn index_handler(
    State(state): State<AppState>,
    body: String,
) -> impl IntoResponse {
    let req = match serde_json::from_str::<IndexRequest>(&body) {
        Ok(r) => {
            tracing::debug!("index request: doc_id={}", r.doc_id);
            r
        }
        Err(e) => {
            tracing::warn!("index parse error: {}", e);
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(json!({ "status": "error", "message": format!("Invalid JSON: {}", e) })),
            ).into_response()
        }
    };

    match state.store.index_document(&req.doc_id, &req.title, &req.body, req.source_url.as_deref()) {
        Ok(()) => {
            tracing::info!("indexed document doc_id={}", req.doc_id);
            Json(json!({ "status": "ok" })).into_response()
        }
        Err(e) => {
            tracing::error!("index document doc_id={} failed: {}", req.doc_id, e);
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "status": "error", "message": e.to_string() })),
            ).into_response()
        }
    }
}

pub async fn batch_index_handler(
    State(state): State<AppState>,
    body: String,
) -> impl IntoResponse {
    let req = match serde_json::from_str::<BatchIndexRequest>(&body) {
        Ok(r) => {
            tracing::debug!("batch index request: {} documents", r.documents.len());
            r
        }
        Err(e) => {
            tracing::warn!("batch index parse error: {}", e);
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
        Ok(()) => {
            tracing::info!("batch indexed {} documents", docs.len());
            Json(json!({ "status": "ok", "count": docs.len() })).into_response()
        }
        Err(e) => {
            tracing::error!("batch index failed: {}", e);
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "status": "error", "message": e.to_string() })),
            ).into_response()
        }
    }
}

pub async fn get_document_handler(
    State(state): State<AppState>,
    body: String,
) -> impl IntoResponse {
    let req = match serde_json::from_str::<GetDocumentRequest>(&body) {
        Ok(r) => {
            tracing::debug!("get document request: doc_id={}", r.doc_id);
            r
        }
        Err(e) => {
            tracing::warn!("get document parse error: {}", e);
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(json!({ "status": "error", "message": format!("Invalid JSON: {}", e) })),
            ).into_response()
        }
    };

    match state.store.get_document(&req.doc_id) {
        Ok(Some(doc)) => {
            tracing::info!("found document doc_id={}", req.doc_id);
            Json(json!(doc)).into_response()
        }
        Ok(None) => {
            tracing::warn!("document not found doc_id={}", req.doc_id);
            (
                axum::http::StatusCode::NOT_FOUND,
                Json(json!({ "status": "error", "message": "document not found" })),
            ).into_response()
        }
        Err(e) => {
            tracing::error!("get document doc_id={} failed: {}", req.doc_id, e);
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "status": "error", "message": e.to_string() })),
            ).into_response()
        }
    }
}

pub async fn health(State(state): State<AppState>) -> Json<serde_json::Value> {
    let store_ok = state.store.search("", 1).is_ok();
    let status = if store_ok { "ok" } else { "degraded" };
    tracing::debug!("health check: {}", status);
    Json(json!({
        "status": status,
        "store": if store_ok { "healthy" } else { "unhealthy" },
    }))
}

pub async fn metrics_handler(
    State(state): State<AppState>,
) -> Json<MetricsSnapshot> {
    let snap = state.store.metrics().snapshot();
    tracing::debug!("metrics: {} indexed, {} searches", snap.documents_indexed, snap.searches_performed);
    Json(snap)
}
