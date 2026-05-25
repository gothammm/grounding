# API Reference

All requests use `Content-Type: application/json`.

## Health

```
GET /health
```

Response:
```json
{
  "status": "ok",
  "store": "healthy"
}
```

## Metrics

```
GET /metrics
```

Response:
```json
{
  "documents_indexed": 42,
  "searches_performed": 7
}
```

## Index a Document

```
POST /index
```

Request body:
```json
{
  "doc_id": "unique-string-123",
  "title": "Document Title",
  "body": "Full text content of the document...",
  "source_url": "https://example.com/doc"
}
```

`source_url` is optional. Omit or set to `null` when there is no meaningful source URL.

Response (success):
```json
{ "status": "ok" }
```

Response (error):
```json
{ "status": "error", "message": "description of what went wrong" }
```

## Batch Index Documents

```
POST /index/batch
```

Request body:
```json
{
  "documents": [
    {
      "doc_id": "doc-1",
      "title": "First Title",
      "body": "First body content...",
      "source_url": "https://example.com/1"
    },
    {
      "doc_id": "doc-2",
      "title": "Second Title",
      "body": "Second body content..."
    }
  ]
}
```

Response (success):
```json
{ "status": "ok", "count": 2 }
```

## Get Document by ID

```
POST /documents
```

Request body:
```json
{
  "doc_id": "unique-string-123"
}
```

Response (found with source_url):
```json
{
  "score": 0.0,
  "doc_id": "unique-string-123",
  "title": "Document Title",
  "snippet": "First 500 characters of the body...",
  "source_url": "https://example.com/doc"
}
```

Response (found without source_url):
```json
{
  "score": 0.0,
  "doc_id": "unique-string-456",
  "title": "Another Document",
  "snippet": "First 500 characters...",
  "source_url": null
}
```

Response (not found):
```json
{ "status": "error", "message": "document not found" }
```

## Query

```
POST /query
```

Request body:
```json
{
  "query": "search terms here",
  "top_k": 5,
  "mode": "hybrid"
}
```

`top_k` defaults to 5 if omitted. `mode` defaults to `"hybrid"` if omitted.

Available modes:
- `"hybrid"` — BM25 keyword search fused with vector embeddings via RRF. Best results. (default)
- `"bm25"` — BM25 keyword search only
- `"vector"` — Vector embedding similarity only (requires embedding model)

Response:
```json
{
  "results": [
    {
      "score": 2.345,
      "doc_id": "unique-string-123",
      "title": "Document Title",
      "snippet": "the first unique passage that contains search terms... For example, the snippet will show the most relevant context passage centered around query term matches rather than just the start of the document.",
      "source_url": "https://example.com/doc"
    },
    {
      "score": 1.234,
      "doc_id": "unique-string-456",
      "title": "Another Document",
      "snippet": "and here is another relevant passage from a different document...",
      "source_url": null
    }
  ]
}
```

Results are ordered by relevance score descending (BM25 for `"bm25"` mode, vector similarity for `"vector"` mode, RRF-fused for `"hybrid"` mode). Each `snippet` contains the most relevant passage (sentences/paragraphs) centered around query term matches, snapped to sentence boundaries — like a Google featured snippet, not just a document preview. Use the `/documents` endpoint to retrieve the full document body. `source_url` is `null` when no source URL was provided during indexing.
