# Architecture Decisions

## Why Tantivy

Tantivy is an embedded full-text search library (like Lucene, but in Rust). It gives us BM25 ranking, full control over scoring parameters, and compiles into a single binary. No separate server process, no external dependency to manage.

## Why Not SQLite FTS

- No vector search path for future V2
- No BM25 scoring control
- Worse performance for large text corpora

## Why Not Quickwit for MVP

Quickwit is designed for clustered log search at scale. Overkill for single-instance deployment. It becomes relevant when customers outgrow the embedded index.

## Distribution Path

1. **MVP**: Embedded Tantivy (one binary, one docker container)
2. **V2** (current): BM25 + fastembed vector embeddings fused via RRF (non-breaking, hybrid query by default)
3. **Scale**: Quickwit backend swap — same Tantivy foundation, same API, just `--backend quickwit`

## Why Vectors Now

BM25 keyword search is precise for exact matches but misses semantic relationships (e.g., "car" ↔ "automobile"). Adding fastembed ONNX embeddings at index time with in-process cosine similarity at query time gives us the best of both worlds:
- **BM25** catches exact terminology matches
- **Vector** catches semantically related concepts
- **RRF fusion** combines both into a single ranked list, no tuning required

Cold-start latency is handled gracefully: on first run the model (~33MB) downloads once to `~/.cache/fastembed/`. Subsequent runs use a cached ONNX Runtime session (~3ms inference).
