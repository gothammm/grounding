use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router, ServiceExt};
use schemars::JsonSchema;
use serde::Deserialize;
use std::sync::Arc;

use crate::store::{IndexDocument, Store};

#[derive(Clone)]
pub struct McpHandler {
    store: Arc<Store>,
}

impl McpHandler {
    pub fn new(store: Store) -> Self {
        Self {
            store: Arc::new(store),
        }
    }

    pub async fn serve_stdio(self) -> anyhow::Result<()> {
        self.serve(rmcp::transport::stdio())
            .await?
            .waiting()
            .await?;
        Ok(())
    }

    pub fn http_service(
        &self,
    ) -> rmcp::transport::streamable_http_server::tower::StreamableHttpService<
        impl rmcp::Service<rmcp::RoleServer>,
        impl rmcp::transport::streamable_http_server::session::SessionManager,
    > {
        use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
        use rmcp::transport::streamable_http_server::tower::{
            StreamableHttpServerConfig, StreamableHttpService,
        };

        let factory = {
            let store = self.store.clone();
            move || Ok::<McpHandler, std::io::Error>(McpHandler {
                store: store.clone(),
            })
        };

        StreamableHttpService::new(
            factory,
            LocalSessionManager::default().into(),
            StreamableHttpServerConfig::default(),
        )
    }
}

#[derive(Deserialize, JsonSchema)]
struct QueryRequest {
    query: String,
    #[serde(default = "default_top_k")]
    top_k: usize,
}

#[derive(Deserialize, JsonSchema)]
struct IndexRequest {
    doc_id: String,
    title: String,
    body: String,
    #[serde(default)]
    source_url: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct GetDocumentRequest {
    doc_id: String,
}

#[derive(Deserialize, JsonSchema)]
struct BatchIndexRequest {
    documents: Vec<IndexRequest>,
}

fn default_top_k() -> usize {
    5
}

#[tool_router(server_handler)]
impl McpHandler {
    #[tool(
        description = "Search the team's indexed documentation and context repository using BM25 ranking to find relevant documents. Returns results sorted by relevance score.

Use this tool when you need to find specific information, answer questions, or retrieve context from your team's indexed knowledge base. It powers Retrieval-Augmented Generation (RAG) workflows — you search for relevant docs, then use the results to ground your responses in factual team knowledge.

Parameters:
- query (string, required): Search terms to match against document titles and body content. Use natural language phrases or keywords — the BM25 ranker handles both well.
- top_k (integer, optional, default=5): Maximum number of results to return. Increase for broader context gathering, decrease for pinpoint retrieval.

Returns: JSON array of matching documents sorted by BM25 score descending. Each result contains:
- score (number): BM25 relevance score. Higher = more relevant. Use this to threshold results (e.g., discard scores below 0.5).
- doc_id (string): Unique document identifier. Pass this to get_document to retrieve the full document.
- title (string): Document title. Useful as a summary/label for the result.
- snippet (string): First 500 characters of the document body. Provides immediate context without a second round-trip.
- source_url (string | null): Original source URL if one was provided during indexing. Useful for attribution or linking back.

Team examples:
- \"Find our deployment configuration docs\" → search_docs(query=\"deployment configuration setup\", top_k=3)
- \"What's our API rate limiting policy\" → search_docs(query=\"API rate limit policy\", top_k=5)
- \"Show me the architecture decisions for the billing service\" → search_docs(query=\"billing service architecture decisions\", top_k=5)
- \"Get recent onboarding documentation\" → search_docs(query=\"onboarding guide new engineers\", top_k=10)

Related tools:
- index_document / batch_index: Use these first to populate the index with your docs
- get_document: After search, call get_document with the doc_id to get the full document body (not just the 500-char snippet)"
    )]
    async fn search_docs(
        &self,
        Parameters(req): Parameters<QueryRequest>,
    ) -> Result<String, String> {
        tracing::info!("search_docs query=\"{}\" top_k={}", req.query, req.top_k);
        match self.store.search(&req.query, req.top_k) {
            Ok(results) => {
                tracing::info!("search_docs query=\"{}\" returned {} results", req.query, results.len());
                serde_json::to_string(&results).map_err(|e| e.to_string())
            }
            Err(e) => {
                tracing::error!("search_docs query=\"{}\" failed: {}", req.query, e);
                Err(e.to_string())
            }
        }
    }

    #[tool(
        description = "Add a single document (documentation page, RFC, runbook, code file, etc.) to the team's search index so it becomes findable via search_docs.

Use this tool when your team has new or updated content that should be retrievable for LLM context. Each piece of content — whether it's a README, API reference, incident postmortem, or onboarding guide — becomes searchable immediately after indexing.

Parameters:
- doc_id (string, required): Unique identifier for this document. Must be unique across your entire index — reusing an existing doc_id will UPSERT (overwrite) the previous document. Use a meaningful slug like \"deployment-runbook-v2\" or \"api-rate-limiting-policy\".
- title (string, required): Human-readable document title. This is what users and LLMs see in search results. Make it descriptive.
- body (string, required): Full text content of the document. Include enough detail so that BM25 search can match relevant queries. This is the text that gets analyzed (tokenized, lowercased, stop-word removed) and indexed for search.
- source_url (string, optional): Original URL where this document lives (e.g., GitHub repo path, Notion page URL, Confluence link). Useful for attribution and allowing users to click through to the source. Omit or set to null when there's no meaningful URL.

Returns: \"ok\" on success, or an error message string on failure (e.g., invalid input, I/O error).

Team examples:
- Index a deployment runbook: index_document(doc_id=\"deploy-runbook\", title=\"Production Deployment Runbook\", body=\"# Deployment\n## Prerequisites\n...\", source_url=\"https://github.com/team/ops/wiki/deploy.md\")
- Index an API reference: index_document(doc_id=\"payments-api-v3\", title=\"Payments API v3 Reference\", body=\"## POST /v3/charges\nCreates a new charge...\", source_url=\"https://docs.team.com/api/payments-v3\")
- Index an incident postmortem: index_document(doc_id=\"postmortem-2026-05-01\", title=\"May 1 Outage Postmortem\", body=\"## Incident Summary\nOn May 1, 2026...\", source_url=\"https://github.com/team/postmortems/2026-05-01.md\")
- Index architecture decision: index_document(doc_id=\"adr-042-database-choice\", title=\"ADR-042: Database Selection for Analytics\", body=\"## Context\nWe need a database...\", source_url=\"https://github.com/team/adrs/0042.md\")

Related tools:
- batch_index: Prefer this when adding 2+ documents (faster, atomic). For bulk imports, send small chunks of 10-20 documents per call.
- search_docs: After indexing, search to verify your document is findable
- get_document: Retrieve the full document later by its doc_id"
    )]
    async fn index_document(
        &self,
        Parameters(req): Parameters<IndexRequest>,
    ) -> Result<String, String> {
        tracing::info!("index_document doc_id={}", req.doc_id);
        match self
            .store
            .index_document(&req.doc_id, &req.title, &req.body, req.source_url.as_deref())
        {
            Ok(()) => {
                tracing::info!("index_document doc_id={} succeeded", req.doc_id);
                Ok("ok".to_string())
            }
            Err(e) => {
                tracing::error!("index_document doc_id={} failed: {}", req.doc_id, e);
                Err(e.to_string())
            }
        }
    }

    #[tool(
        description = "Retrieve the full details of a single document from the team's index by its unique doc_id. This is a direct lookup (not a search) — you need to know the exact doc_id.

Use this tool when you have a specific document ID (from a search result, from another tool, or from your knowledge) and you need the complete document content. This is the second step in the common \"search → retrieve\" pattern: first find relevant docs with search_docs, then fetch the full content with get_document.

Parameters:
- doc_id (string, required): The unique document identifier that was provided when the document was indexed. Must match exactly.

Returns on success: JSON object with the document data:
- score (number): Always 0.0 — this is a direct retrieval, not a search, so score is not applicable.
- doc_id (string): The requested document ID (echoed back for confirmation).
- title (string): The document title.
- snippet (string): First 500 characters of the document body — enough to confirm context without reading the entire body.
- source_url (string | null): Source URL if one was provided when indexing; null otherwise.

Returns on failure:
- Error message \"document not found\" if no document matches the given doc_id.
- Error message describing the failure for I/O errors or index corruption.

Team examples:
- Retrieve a known ADR: get_document(doc_id=\"adr-042-database-choice\") → Gets the full ADR-042 document
- Verify indexing: Get a document you just indexed to confirm body was stored correctly
- Fetch full context: search_docs returns snippets → you find doc_id=\"deploy-runbook\" → call get_document for the complete runbook text
- Process batch results: batch_index returns count → call get_document for specific docs to verify

Related tools:
- search_docs: Use first to discover relevant doc_ids, then call get_document for full content
- index_document: Used to create the document that get_document retrieves"
    )]
    async fn get_document(
        &self,
        Parameters(req): Parameters<GetDocumentRequest>,
    ) -> Result<String, String> {
        tracing::info!("get_document doc_id={}", req.doc_id);
        match self.store.get_document(&req.doc_id) {
            Ok(Some(doc)) => {
                tracing::info!("get_document doc_id={} found", req.doc_id);
                serde_json::to_string(&doc).map_err(|e| e.to_string())
            }
            Ok(None) => {
                tracing::warn!("get_document doc_id={} not found", req.doc_id);
                Err("document not found".to_string())
            }
            Err(e) => {
                tracing::error!("get_document doc_id={} failed: {}", req.doc_id, e);
                Err(e.to_string())
            }
        }
    }

    #[tool(
        description = "Add multiple documents to the team's search index in a single atomic batch operation. Significantly faster than calling index_document repeatedly for bulk imports.

Use this tool when your team needs to index multiple documents at once — such as importing an entire documentation set, onboarding a new project's knowledge base, or syncing from an external CMS. The batch operation is atomic: either ALL documents are indexed successfully, or NONE are (on failure), preventing partial updates.

Parameters:
- documents (array, required): Array of document objects to index. Each document requires:
  - doc_id (string): Unique identifier. Duplicate IDs within the same batch are allowed but the last one wins (no deduplication).
  - title (string): Human-readable title for search result display.
  - body (string): Full text content to index and make searchable.
  - source_url (string, optional): Source URL for attribution. Omit or set to null if not applicable.

Returns on success: JSON object {\"count\": N} where N is the number of documents successfully indexed. All documents in the batch will have been indexed on success.

Returns on failure: Error message describing the failure. No documents from the batch will be indexed on failure (atomic rollback).

Performance guidance:
- Batch size: Keep batches small (10-20 documents max) to avoid timeouts and reduce atomic rollback risk on failure. Large batches may take seconds to commit and fail entirely if any single document errors.
- Atomicity: The batch is committed as a single index operation on the Tantivy writer. A failure in any document causes the entire batch to roll back.
- When to use: Prefer batch_index for 2+ documents; use index_document for single updates.

Team examples:
- Import project documentation: batch_index(documents=[{doc_id=\"api-ref\", title=\"API Reference\", body=\"...\"}, {doc_id=\"setup-guide\", title=\"Setup Guide\", body=\"...\"}, {doc_id=\"contributing\", title=\"Contributing Guidelines\", body=\"...\"}])
- Sync from a CMS: Pull docs from Confluence/Notion and batch-index them daily. Each page becomes a document with its URL as source_url.
- Onboard a new microservice: Index all README files, API specs, and runbooks for a new service in one call.
- Migrate knowledge bases: When switching from another search tool, export docs as JSON and batch-import them here.
- Bulk update after restructuring: If doc titles or paths change, re-index all affected docs in one batch.

Related tools:
- index_document: Use for single document indexing (prefer batch_index for 5+ docs)
- search_docs: After batch indexing, search to verify the new documents are findable
- get_document: Retrieve individual documents by doc_id to confirm batch content"
    )]
    async fn batch_index(
        &self,
        Parameters(req): Parameters<BatchIndexRequest>,
    ) -> Result<String, String> {
        let docs: Vec<IndexDocument> = req
            .documents
            .into_iter()
            .map(|d| IndexDocument {
                doc_id: d.doc_id,
                title: d.title,
                body: d.body,
                source_url: d.source_url,
            })
            .collect();

        tracing::info!("batch_index {} documents", docs.len());
        match self.store.index_documents(&docs) {
            Ok(()) => {
                tracing::info!("batch_index {} documents succeeded", docs.len());
                Ok(format!("{{\"count\":{}}}", docs.len()))
            }
            Err(e) => {
                tracing::error!("batch_index failed: {}", e);
                Err(e.to_string())
            }
        }
    }
}
