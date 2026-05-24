use super::types::IndexDocument;
use super::index::Store;
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

    store.index_document(
        "doc1",
        "Test Document",
        "This is a test document about searching and indexing",
        Some("https://example.com/doc1"),
    ).unwrap();

    let results = store.search("searching", 5).unwrap();
    assert!(!results.is_empty());
    assert!(results[0].title.contains("Test Document"));
}

#[test]
fn test_search_returns_score_ordered() {
    let (store, _tmp) = setup_store();

    store.index_document("doc1", "Rust", "Rust is a systems programming language", Some("https://example.com/rust")).unwrap();
    store.index_document("doc2", "Python", "Python is a scripting language", Some("https://example.com/python")).unwrap();

    let results = store.search("language", 5).unwrap();
    assert!(results.len() >= 2);
    for i in 1..results.len() {
        assert!(results[i-1].score >= results[i].score);
    }
}

#[test]
fn test_snippet_truncation() {
    let (store, _tmp) = setup_store();

    let long_body = "word ".repeat(200);
    store.index_document("doc1", "Test", &long_body, Some("https://example.com")).unwrap();

    let results = store.search("word", 5).unwrap();
    assert!(!results.is_empty());
    assert!(results[0].snippet.len() <= 500);
}

#[test]
fn test_batch_indexing() {
    let (store, _tmp) = setup_store();

    store.index_documents(&[
        IndexDocument {
            doc_id: "a".into(),
            title: "Alpha".into(),
            body: "First letter of the Greek alphabet".into(),
            source_url: Some("https://example.com/alpha".into()),
        },
        IndexDocument {
            doc_id: "b".into(),
            title: "Beta".into(),
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

    store.index_document("m1", "X", "some content", Some("https://x.com")).unwrap();
    store.search("content", 5).unwrap();

    let snap = store.metrics().snapshot();
    assert_eq!(snap.documents_indexed, 1);
    assert_eq!(snap.searches_performed, 1);
}
