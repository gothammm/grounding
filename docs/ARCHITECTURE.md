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
│   └── types.rs    # SearchResult, IndexDocument, Metrics
└── api/
    ├── mod.rs      # Router setup, serve()
    ├── handlers.rs # HTTP handlers
    └── models.rs   # Request DTOs
```

## Key Design Decisions

- **Single binary**: Tantivy is an embedded library — no separate server process
- **Disk-persistent**: Data survives restart. Index written to `./data/index/`
- **No vector search in MVP**: BM25 + field boosting is sufficient for code/docs retrieval. Vectors added in V2
- **Distribution path**: Swap storage backend to Quickwit (clustered Tantivy on S3) when needed — same API
