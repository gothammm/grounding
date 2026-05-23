# Setup

## Build

```bash
cargo build --release
```

## Run

```bash
cargo run -- serve --data-dir ./data --port 8080
```

Or with the release binary:
```bash
./target/release/grounding serve --data-dir ./data
```

### Environment Variables

These override the corresponding CLI flags:

| Variable | Default | Description |
|----------|---------|-------------|
| `GROUNDING_DATA_DIR` | `./data` | Path to data directory |
| `GROUNDING_PORT` | `8080` | HTTP server port |

Example:
```bash
GROUNDING_DATA_DIR=/var/lib/grounding GROUNDING_PORT=3000 ./target/release/grounding serve
```

## Docker

```dockerfile
FROM scratch
COPY grounding /
VOLUME /data
EXPOSE 8080
ENTRYPOINT ["/grounding", "serve", "--data-dir", "/data"]
```

Build:
```bash
cargo build --release --target x86_64-unknown-linux-musl
```

## Test with curl

### Index a document
```bash
curl -X POST http://localhost:8080/index \
  -H "Content-Type: application/json" \
  -d '{"doc_id": "1", "title": "Hello", "body": "World wide web hello", "source_url": "https://example.com"}'
```

### Batch index documents
```bash
curl -X POST http://localhost:8080/index/batch \
  -H "Content-Type: application/json" \
  -d '{"documents": [{"doc_id":"2","title":"Foo","body":"Foo bar baz","source_url":"https://example.com/foo"}]}'
```

### Get document by ID
```bash
curl -X POST http://localhost:8080/documents \
  -H "Content-Type: application/json" \
  -d '{"doc_id": "1"}'
```

### Query
```bash
curl -X POST http://localhost:8080/query \
  -H "Content-Type: application/json" \
  -d '{"query": "hello world", "top_k": 3}'
```

### Metrics
```bash
curl http://localhost:8080/metrics
```

## Data Directory

The `--data-dir` directory contains all persisted state:

```
./data/
└── index/     # Tantivy segments (survives restart)
```

Delete this directory to reset all data.
