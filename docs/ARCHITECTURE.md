# Architecture

Grounding is a single-binary retrieval engine for LLM context. No external database required.

## Data Layout

```
./data/
└── index/          # Tantivy segments (BM25 inverted index)
    ├── meta.json
    ├── segment_1/
    └── segment_2/
```

## Module Structure

```
src/
├── main.rs         # CLI entry (clap: `grounding serve --data-dir ./data`)
├── lib.rs          # Module declarations, wires up serve()
├── config.rs       # Config struct (data_dir, index_dir)
├── store/
│   ├── mod.rs      # Re-exports
│   ├── index.rs    # Tantivy index: schema, search, indexing
│   ├── types.rs    # SearchResult, IndexDocument, Metrics
│   └── embedding.rs# Vector embedding store (fastembed ONNX, cosine similarity)
└── api/
    ├── mod.rs      # Router setup, serve()
    ├── handlers.rs # HTTP handlers
    └── models.rs   # Request DTOs
```

## Key Design Decisions

- **Single binary**: Tantivy is an embedded library — no separate server process
- **BM25 + vector hybrid search**: BM25 keyword search fused with fastembed vector embeddings via RRF (Reciprocal Rank Fusion). Default mode for all queries. BM25-only available via API `mode` parameter
- **Disk-persistent**: Data survives restart. Index written to `./data/index/`. Embeddings stored in Tantivy `BYTES | STORED` field alongside text
- **Distribution path**: Swap storage backend to Quickwit (clustered Tantivy on S3) when needed — same API
