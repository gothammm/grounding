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
2. **V2**: Add vector embeddings to same index schema (non-breaking hybrid query)
3. **Scale**: Quickwit backend swap — same Tantivy foundation, same API, just `--backend quickwit`

## What About Vectors

BM25 + field boosting provides strong precision for code and documentation retrieval. Vector search adds embedding infrastructure and cold-start latency with marginal gains for this domain. Vectors are additive in V2 without API breakage.
