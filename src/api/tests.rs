use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use std::sync::Arc;
use tower::ServiceExt;

use crate::api::handlers::{self, AppState};
use crate::api::serve;
use crate::config::Config;
use crate::store::Store;
use tempfile::TempDir;

fn setup() -> (Store, TempDir) {
    let tmp = TempDir::new().unwrap();
    let config = Config::new(tmp.path());
    let store = Store::open(&config).unwrap();
    (store, tmp)
}

fn app(store: Store) -> Router {
    let state = AppState { store: Arc::new(store) };
    Router::new()
        .route("/health", axum::routing::get(handlers::health))
        .route("/index", axum::routing::post(handlers::index_handler))
        .route("/index/batch", axum::routing::post(handlers::batch_index_handler))
        .route("/documents", axum::routing::post(handlers::get_document_handler))
        .route("/query", axum::routing::post(handlers::query_handler))
        .route("/metrics", axum::routing::get(handlers::metrics_handler))
        .with_state(state)
}

#[tokio::test]
async fn test_health() {
    let (store, _tmp) = setup();
    let app = app(store);

    let response = app
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
}

#[tokio::test]
async fn test_index_and_query() {
    let (store, _tmp) = setup();
    let app = app(store);

    let index_resp = app
        .oneshot(
            Request::builder()
                .uri("/index")
                .method("POST")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"doc_id":"1","title":"Hello","body":"world wide web hello","source_url":"https://example.com"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(index_resp.status(), StatusCode::OK);

    let query_resp = app
        .oneshot(
            Request::builder()
                .uri("/query")
                .method("POST")
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"query":"hello world","top_k":3}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(query_resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(query_resp.into_body(), 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(!json["results"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_batch_index() {
    let (store, _tmp) = setup();
    let app = app(store);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/index/batch")
                .method("POST")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"documents":[{"doc_id":"a","title":"A","body":"alpha","source_url":"https://a.com"}]}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["count"], 1);
}

#[tokio::test]
async fn test_get_document() {
    let (store, _tmp) = setup();
    let app = app(store);

    app.oneshot(
        Request::builder()
            .uri("/index")
            .method("POST")
            .header("Content-Type", "application/json")
            .body(Body::from(
                r#"{"doc_id":"d1","title":"Doc","body":"some content here","source_url":"https://example.com"}"#,
            ))
            .unwrap(),
    )
    .await
    .unwrap();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/documents")
                .method("POST")
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"doc_id":"d1"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["title"], "Doc");
}

#[tokio::test]
async fn test_get_document_not_found() {
    let (store, _tmp) = setup();
    let app = app(store);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/documents")
                .method("POST")
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"doc_id":"nonexistent"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_metrics() {
    let (store, _tmp) = setup();
    let app = app(store);

    let resp = app
        .oneshot(Request::builder().uri("/metrics").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["documents_indexed"], 0);
    assert_eq!(json["searches_performed"], 0);
}
