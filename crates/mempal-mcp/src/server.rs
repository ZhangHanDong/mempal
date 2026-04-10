use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use mempal_core::{
    db::Database,
    types::{Drawer, SourceType},
    utils::{build_drawer_id, current_timestamp, source_file_or_synthetic},
};
use mempal_embed::EmbedderFactory;
use mempal_search::{resolve_route, search_by_vector};
use rmcp::{
    ErrorData, Json, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
};

use crate::tools::{
    DeleteRequest, DeleteResponse, IngestRequest, IngestResponse, ScopeCount, SearchRequest,
    SearchResponse, SearchResultDto, StatusResponse, TaxonomyEntryDto, TaxonomyRequest,
    TaxonomyResponse,
};

#[derive(Clone)]
pub struct MempalMcpServer {
    db_path: PathBuf,
    embedder_factory: Arc<dyn EmbedderFactory>,
    tool_router: ToolRouter<Self>,
}

impl MempalMcpServer {
    pub fn new(db_path: PathBuf, config: mempal_core::config::Config) -> Self {
        Self::new_with_factory(
            db_path,
            Arc::new(mempal_embed::ConfiguredEmbedderFactory::new(config)),
        )
    }

    pub fn new_with_factory(db_path: PathBuf, embedder_factory: Arc<dyn EmbedderFactory>) -> Self {
        Self {
            db_path,
            embedder_factory,
            tool_router: Self::tool_router(),
        }
    }

    pub async fn serve_stdio(
        self,
    ) -> anyhow::Result<rmcp::service::RunningService<rmcp::RoleServer, Self>> {
        self.serve(rmcp::transport::stdio())
            .await
            .context("failed to initialize MCP stdio transport")
    }

    fn open_db(&self) -> std::result::Result<Database, ErrorData> {
        Database::open(&self.db_path).map_err(|error| {
            ErrorData::internal_error(format!("failed to open database: {error}"), None)
        })
    }
}

#[tool_router(router = tool_router)]
impl MempalMcpServer {
    #[tool(
        name = "mempal_status",
        description = "Return schema version, drawer counts, taxonomy counts, database size, scope breakdown, the AAAK format spec, and the memory protocol. Call once at session start if you haven't seen the protocol yet."
    )]
    async fn mempal_status(&self) -> std::result::Result<Json<StatusResponse>, ErrorData> {
        let db = self.open_db()?;
        let schema_version = db.schema_version().map_err(db_error)?;
        let drawer_count = db.drawer_count().map_err(db_error)?;
        let taxonomy_count = db.taxonomy_count().map_err(db_error)?;
        let db_size_bytes = db.database_size_bytes().map_err(db_error)?;
        let scopes = db
            .scope_counts()
            .map_err(db_error)?
            .into_iter()
            .map(|(wing, room, drawer_count)| ScopeCount {
                wing,
                room,
                drawer_count,
            })
            .collect();

        Ok(Json(StatusResponse {
            schema_version,
            drawer_count,
            taxonomy_count,
            db_size_bytes,
            scopes,
            aaak_spec: mempal_aaak::generate_spec(),
            memory_protocol: mempal_core::protocol::MEMORY_PROTOCOL.to_string(),
        }))
    }

    #[tool(
        name = "mempal_search",
        description = "Search persistent project memory via vector embedding with optional wing/room filters. PREFER THIS over grepping files or guessing from general knowledge when answering ANY project-specific question — past decisions, design rationale, implementation details, bug history, how a component works, why something was built a certain way, or any other project knowledge. Every result includes drawer_id and source_file for citation."
    )]
    async fn mempal_search(
        &self,
        Parameters(request): Parameters<SearchRequest>,
    ) -> std::result::Result<Json<SearchResponse>, ErrorData> {
        let embedder = self.embedder_factory.build().await.map_err(|error| {
            ErrorData::internal_error(format!("failed to build embedder: {error}"), None)
        })?;
        let query_vector = embedder
            .embed(&[request.query.as_str()])
            .await
            .map_err(|error| ErrorData::internal_error(format!("embedding failed: {error}"), None))?
            .into_iter()
            .next()
            .ok_or_else(|| ErrorData::internal_error("embedder returned no query vector", None))?;
        let db = self.open_db()?;
        let route = resolve_route(
            &db,
            &request.query,
            request.wing.as_deref(),
            request.room.as_deref(),
        )
        .map_err(|error| ErrorData::internal_error(format!("routing failed: {error}"), None))?;
        let results = search_by_vector(&db, &query_vector, route, request.top_k.unwrap_or(10))
            .map_err(|error| ErrorData::internal_error(format!("search failed: {error}"), None))?;

        Ok(Json(SearchResponse {
            results: results.into_iter().map(SearchResultDto::from).collect(),
        }))
    }

    #[tool(
        name = "mempal_ingest",
        description = "Persist a decision, bug fix, or design insight to project memory. Call this when a decision is reached in conversation — include the rationale, not just the outcome. Wing is required; let mempal auto-route the room. Set dry_run=true to preview the drawer_id without writing."
    )]
    async fn mempal_ingest(
        &self,
        Parameters(request): Parameters<IngestRequest>,
    ) -> std::result::Result<Json<IngestResponse>, ErrorData> {
        let room = request.room.as_deref();
        let drawer_id = build_drawer_id(&request.wing, room, &request.content);

        if request.dry_run.unwrap_or(false) {
            return Ok(Json(IngestResponse { drawer_id }));
        }

        let embedder = self.embedder_factory.build().await.map_err(|error| {
            ErrorData::internal_error(format!("failed to build embedder: {error}"), None)
        })?;
        let vector = embedder
            .embed(&[request.content.as_str()])
            .await
            .map_err(|error| ErrorData::internal_error(format!("embedding failed: {error}"), None))?
            .into_iter()
            .next()
            .ok_or_else(|| ErrorData::internal_error("embedder returned no vector", None))?;
        let db = self.open_db()?;

        if !db.drawer_exists(&drawer_id).map_err(db_error)? {
            let source_file = source_file_or_synthetic(&drawer_id, request.source.as_deref());
            db.insert_drawer(&Drawer {
                id: drawer_id.clone(),
                content: request.content,
                wing: request.wing,
                room: request.room,
                source_file: Some(source_file),
                source_type: SourceType::Manual,
                added_at: current_timestamp(),
                chunk_index: Some(0),
            })
            .map_err(db_error)?;
            db.insert_vector(&drawer_id, &vector).map_err(db_error)?;
        }

        Ok(Json(IngestResponse { drawer_id }))
    }

    #[tool(
        name = "mempal_delete",
        description = "Soft-delete a drawer by ID. The drawer is marked with a deleted_at timestamp and excluded from search results, but not physically removed. Use the CLI `mempal purge` to permanently remove soft-deleted drawers. Returns the drawer_id and whether it was found."
    )]
    async fn mempal_delete(
        &self,
        Parameters(request): Parameters<DeleteRequest>,
    ) -> std::result::Result<Json<DeleteResponse>, ErrorData> {
        let db = self.open_db()?;
        let deleted = db
            .soft_delete_drawer(&request.drawer_id)
            .map_err(db_error)?;
        let message = if deleted {
            format!("drawer {} soft-deleted", request.drawer_id)
        } else {
            format!("drawer {} not found or already deleted", request.drawer_id)
        };
        Ok(Json(DeleteResponse {
            drawer_id: request.drawer_id,
            deleted,
            message,
        }))
    }

    #[tool(
        name = "mempal_taxonomy",
        description = "List or edit wing/room taxonomy entries that drive query routing keywords."
    )]
    async fn mempal_taxonomy(
        &self,
        Parameters(request): Parameters<TaxonomyRequest>,
    ) -> std::result::Result<Json<TaxonomyResponse>, ErrorData> {
        let db = self.open_db()?;
        match request.action.as_str() {
            "list" => {
                let entries = db
                    .taxonomy_entries()
                    .map_err(db_error)?
                    .into_iter()
                    .map(TaxonomyEntryDto::from)
                    .collect();
                Ok(Json(TaxonomyResponse {
                    action: "list".to_string(),
                    entries,
                }))
            }
            "edit" => {
                let wing = request
                    .wing
                    .ok_or_else(|| ErrorData::invalid_params("missing wing", None))?;
                let room = request
                    .room
                    .ok_or_else(|| ErrorData::invalid_params("missing room", None))?;
                let keywords = request
                    .keywords
                    .ok_or_else(|| ErrorData::invalid_params("missing keywords", None))?;
                let entry = mempal_core::types::TaxonomyEntry {
                    wing,
                    room,
                    display_name: None,
                    keywords,
                };
                db.upsert_taxonomy_entry(&entry).map_err(db_error)?;
                Ok(Json(TaxonomyResponse {
                    action: "edit".to_string(),
                    entries: vec![TaxonomyEntryDto::from(entry)],
                }))
            }
            action => Err(ErrorData::invalid_params(
                format!("unsupported taxonomy action: {action}"),
                None,
            )),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for MempalMcpServer {
    fn get_info(&self) -> ServerInfo {
        // MCP spec: `instructions` is auto-injected into the LLM system prompt
        // by most clients at connection time. Putting the memory protocol here
        // means every client (Claude Code, Codex, Cursor, Continue, ...) sees
        // it without needing to call any tool first. This is the primary
        // mechanism; `mempal_status` keeps the same text as a fallback/reference.
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(mempal_core::protocol::MEMORY_PROTOCOL)
    }
}

fn db_error(error: impl std::fmt::Display) -> ErrorData {
    ErrorData::internal_error(format!("{error}"), None)
}
