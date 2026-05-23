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
      "body": "Second body content...",
      "source_url": "https://example.com/2"
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

Response (found):
```json
{
  "score": 0.0,
  "doc_id": "unique-string-123",
  "title": "Document Title",
  "snippet": "First 500 characters of the body...",
  "source_url": "https://example.com/doc"
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
  "top_k": 5
}
```

`top_k` defaults to 5 if omitted.

Response:
```json
{
  "results": [
    {
      "score": 2.345,
      "doc_id": "unique-string-123",
      "title": "Document Title",
      "snippet": "First 500 characters of the body...",
      "source_url": "https://example.com/doc"
    }
  ]
}
```

Results are ordered by BM25 score descending.
