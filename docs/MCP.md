# MCP Interface

Grounding exposes a **Model Context Protocol (MCP)** interface alongside the HTTP API. MCP allows LLM agents (Claude Code, Claude Desktop, Cursor, etc.) to use grounding as a retrieval tool directly.

## Transports

### 1. Stdio (local agents)

```bash
grounding mcp --data-dir ./data
```

This starts the MCP server on stdin/stdout — the standard transport for local tools like **Claude Code**.

#### Claude Desktop configuration

Add to your `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "grounding": {
      "command": "/path/to/grounding",
      "args": ["mcp", "--data-dir", "/path/to/data"]
    }
  }
}
```

### 2. Streamable HTTP (remote agents)

Available at `/mcp` on the HTTP server. Start the server normally:

```bash
grounding serve --data-dir ./data --port 8080
```

MCP clients connect to `http://localhost:8080/mcp` using the [Streamable HTTP](https://spec.modelcontextprotocol.io/specification/2025-03-26/basic/transports/streamable-http/) transport.

## Tools

| Tool | Description |
|------|-------------|
| `search_docs` | Search indexed documents using BM25 ranking |
| `index_document` | Index a single document |
| `get_document` | Retrieve a document by its ID |
| `batch_index` | Index multiple documents in batch |

### search_docs

```
Arguments:
  query (string):   Search terms
  top_k  (number):  Number of results (default: 5)

Returns: JSON array of SearchResult objects
```

### index_document

```
Arguments:
  doc_id     (string):  Unique document identifier
  title      (string):  Document title
  body       (string):  Full text content
  source_url (string):  Source URL (optional — omit for content without a meaningful source)

Returns: "ok" on success
```

### get_document

```
Arguments:
  doc_id (string): Document ID to retrieve

Returns: SearchResult JSON object, or error "document not found"
```

### batch_index

```
Arguments:
  documents (array): Array of {doc_id, title, body, source_url?} objects

Returns: {"count": N}
```

## Testing with curl (Streamable HTTP)

MCP uses JSON-RPC 2.0 over the Streamable HTTP transport. Connect with an MCP client SDK for proper session handling, or use `grounding mcp` on stdio for direct tool access.
