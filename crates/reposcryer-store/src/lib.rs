use anyhow::{Context, Result, anyhow, bail};
use kuzu::{Connection, Database, SystemConfig, Value};
use reposcryer_core::{
    CodeChunk, CodeFile, Confidence, Evidence, FileChange, FileChangeKind, FileDependency,
    FileExplanation, FileFingerprint, FileFingerprintRecord, FileId, FileImpact, FileNeighborhood,
    FileStatus, ImpactedFile, Import, ImportId, IncrementalIndexPlan, IndexContext, IndexRun,
    IndexRunStatus, IndexStats, IndexWarning, Language, ParsedFile, RepoStatus, RunId, ScanResult,
    ScopeId, Symbol, SymbolId, SymbolKind, WarningId, file_id_from_relative_path, stable_hash,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const STORE_SCHEMA_VERSION: &str = "phase3-kuzu-v1";

pub trait GraphStore {
    fn init_schema(&self) -> Result<()>;
    fn begin_index_run(&self, ctx: &IndexContext) -> Result<IndexRun>;
    fn complete_index_run(&self, run_id: &RunId, stats: &IndexStats) -> Result<()>;
    fn fail_index_run(&self, run_id: &RunId, error: &str) -> Result<()>;

    fn load_file_fingerprints(&self, scope_id: &ScopeId) -> Result<Vec<FileFingerprintRecord>>;
    fn build_incremental_plan(
        &self,
        scan: &ScanResult,
        ctx: &IndexContext,
    ) -> Result<IncrementalIndexPlan>;

    fn replace_file_subgraph(
        &self,
        file: &CodeFile,
        parsed: &ParsedFile,
        run_id: &RunId,
    ) -> Result<()>;
    fn mark_file_deleted(&self, file_id: &FileId, run_id: &RunId) -> Result<()>;

    fn repo_status(&self, scan: &ScanResult, ctx: &IndexContext) -> Result<RepoStatus>;
    fn changed_files(&self, scan: &ScanResult, ctx: &IndexContext) -> Result<Vec<FileChange>>;
    fn rebuild_scope_import_edges(&self, ctx: &IndexContext) -> Result<usize>;
    fn explain_file(
        &self,
        ctx: &IndexContext,
        relative_path: &Path,
    ) -> Result<Option<FileExplanation>>;
    fn file_neighbors(
        &self,
        ctx: &IndexContext,
        relative_path: &Path,
    ) -> Result<Option<FileNeighborhood>>;
    fn file_impact(&self, ctx: &IndexContext, relative_path: &Path) -> Result<Option<FileImpact>>;
}

#[derive(Debug, Clone)]
pub struct KuzuGraphStore {
    db_path: PathBuf,
}

impl KuzuGraphStore {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            db_path: path.as_ref().to_path_buf(),
        }
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub fn reset_database(&self) -> Result<()> {
        if self.db_path.exists() {
            if self.db_path.is_dir() {
                fs::remove_dir_all(&self.db_path)
            } else {
                fs::remove_file(&self.db_path)
            }
            .with_context(|| format!("failed to remove {}", self.db_path.display()))?;
        }
        Ok(())
    }

    fn stored_schema_version(&self) -> Option<String> {
        let rows = self
            .fetch_rows("MATCH (m:SchemaMeta {key: 'schema_version'}) RETURN m.value;")
            .ok()?;
        let row = rows.into_iter().next()?;
        string_at(&row, 0).ok()
    }

    fn with_conn<T>(&self, f: impl FnOnce(&Connection<'_>) -> Result<T>) -> Result<T> {
        if let Some(parent) = self.db_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let config = SystemConfig::default()
            .max_db_size(16 * 1024 * 1024 * 1024)
            .max_num_threads(1);
        let db = Database::new(&self.db_path, config)
            .map_err(|error| anyhow!("failed to open kuzu database: {error}"))?;
        let conn = Connection::new(&db)
            .map_err(|error| anyhow!("failed to connect to kuzu database: {error}"))?;
        f(&conn)
    }

    fn query(&self, query: &str) -> Result<()> {
        self.with_conn(|conn| {
            conn.query(query)
                .map_err(|error| anyhow!("kuzu query failed: {error}; query={query}"))?;
            Ok(())
        })
    }

    fn fetch_rows(&self, query: &str) -> Result<Vec<Vec<Value>>> {
        self.with_conn(|conn| {
            let result = conn
                .query(query)
                .map_err(|error| anyhow!("kuzu query failed: {error}; query={query}"))?;
            Ok(result.collect())
        })
    }

    fn ensure_context_nodes(&self, ctx: &IndexContext) -> Result<()> {
        let now = now_string();
        self.query(&format!(
            "MERGE (p:Project {{project_id: {project_id}}}) \
             ON CREATE SET p.repo_id = {repo_id}, p.root_path = {root_path}, p.created_at = {now}, p.updated_at = {now} \
             ON MATCH SET p.repo_id = {repo_id}, p.root_path = {root_path}, p.updated_at = {now};",
            project_id = lit(&ctx.project_id.0),
            repo_id = lit(&ctx.repo_id.0),
            root_path = lit(&ctx.repo_root.to_string_lossy()),
            now = lit(&now),
        ))?;
        self.query(&format!(
            "MERGE (w:Worktree {{worktree_id: {worktree_id}}}) \
             ON CREATE SET w.project_id = {project_id}, w.root_path = {root_path}, w.created_at = {now}, w.updated_at = {now} \
             ON MATCH SET w.project_id = {project_id}, w.root_path = {root_path}, w.updated_at = {now};",
            worktree_id = lit(&ctx.worktree_id.0),
            project_id = lit(&ctx.project_id.0),
            root_path = lit(&ctx.repo_root.to_string_lossy()),
            now = lit(&now),
        ))?;
        self.query(&format!(
            "MERGE (s:IndexScope {{scope_id: {scope_id}}}) \
             ON CREATE SET s.project_id = {project_id}, s.worktree_id = {worktree_id}, s.repo_root = {repo_root}, s.parser_version = {parser_version}, s.graph_schema_version = {schema_version}, s.chunker_version = {chunker_version}, s.created_at = {now}, s.updated_at = {now} \
             ON MATCH SET s.project_id = {project_id}, s.worktree_id = {worktree_id}, s.repo_root = {repo_root}, s.parser_version = {parser_version}, s.graph_schema_version = {schema_version}, s.chunker_version = {chunker_version}, s.updated_at = {now};",
            scope_id = lit(&ctx.scope_id.0),
            project_id = lit(&ctx.project_id.0),
            worktree_id = lit(&ctx.worktree_id.0),
            repo_root = lit(&ctx.repo_root.to_string_lossy()),
            parser_version = lit(&ctx.parser_version),
            schema_version = lit(&ctx.schema_version),
            chunker_version = lit(&ctx.chunker_version),
            now = lit(&now),
        ))?;
        self.query(&format!(
            "MATCH (p:Project {{project_id: {project_id}}}), (w:Worktree {{worktree_id: {worktree_id}}}) \
             MERGE (p)-[:HAS_WORKTREE]->(w);",
            project_id = lit(&ctx.project_id.0),
            worktree_id = lit(&ctx.worktree_id.0),
        ))?;
        self.query(&format!(
            "MATCH (w:Worktree {{worktree_id: {worktree_id}}}), (s:IndexScope {{scope_id: {scope_id}}}) \
             MERGE (w)-[:HAS_SCOPE]->(s);",
            worktree_id = lit(&ctx.worktree_id.0),
            scope_id = lit(&ctx.scope_id.0),
        ))?;
        Ok(())
    }

    fn remove_file_subgraph(&self, file_id: &FileId) -> Result<()> {
        self.remove_file_dependency_edges(file_id, false)?;
        self.query(&format!(
            "MATCH (f:File {{file_id: {file_id}}})-[:HAS_CHUNK]->(c:Chunk) DETACH DELETE c;",
            file_id = lit(&file_id.0),
        ))?;
        self.query(&format!(
            "MATCH (f:File {{file_id: {file_id}}})-[:DEFINES]->(s:Symbol) DETACH DELETE s;",
            file_id = lit(&file_id.0),
        ))?;
        self.query(&format!(
            "MATCH (f:File {{file_id: {file_id}}})-[:HAS_IMPORT]->(i:Import) DETACH DELETE i;",
            file_id = lit(&file_id.0),
        ))?;
        self.query(&format!(
            "MATCH (f:File {{file_id: {file_id}}})-[:HAS_WARNING]->(w:Warning) DETACH DELETE w;",
            file_id = lit(&file_id.0),
        ))?;
        Ok(())
    }

    fn remove_file_dependency_edges(&self, file_id: &FileId, include_incoming: bool) -> Result<()> {
        self.query(&format!(
            "MATCH (f:File {{file_id: {file_id}}})-[r:IMPORTS_FILE]->(:File) DELETE r;",
            file_id = lit(&file_id.0),
        ))?;
        if include_incoming {
            self.query(&format!(
                "MATCH (:File)-[r:IMPORTS_FILE]->(f:File {{file_id: {file_id}}}) DELETE r;",
                file_id = lit(&file_id.0),
            ))?;
        }
        Ok(())
    }

    fn run_context(&self, run_id: &RunId) -> Result<(IndexContext, String)> {
        let rows = self.fetch_rows(&format!(
            "MATCH (r:IndexRun {{run_id: {run_id}}}) RETURN r.project_id, r.worktree_id, r.scope_id, r.repo_id, r.repo_root, r.parser_version, r.graph_schema_version, r.chunker_version;",
            run_id = lit(&run_id.0),
        ))?;
        let row = rows
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("index run {} not found", run_id.0))?;
        let ctx = IndexContext {
            project_id: reposcryer_core::ProjectId(string_at(&row, 0)?),
            worktree_id: reposcryer_core::WorktreeId(string_at(&row, 1)?),
            scope_id: ScopeId(string_at(&row, 2)?),
            repo_id: reposcryer_core::RepoId(string_at(&row, 3)?),
            repo_root: PathBuf::from(string_at(&row, 4)?),
            parser_version: string_at(&row, 5)?,
            schema_version: string_at(&row, 6)?,
            chunker_version: string_at(&row, 7)?,
        };
        Ok((ctx, run_id.0.clone()))
    }

    fn touch_file(&self, file_id: &FileId, run_id: &RunId) -> Result<()> {
        let touched_at = now_string();
        self.query(&format!(
            "MATCH (r:IndexRun {{run_id: {run_id}}}), (f:File {{file_id: {file_id}}}) \
             MERGE (r)-[t:TOUCHED_FILE]->(f) \
             ON CREATE SET t.touched_at = {touched_at} \
             ON MATCH SET t.touched_at = {touched_at};",
            run_id = lit(&run_id.0),
            file_id = lit(&file_id.0),
            touched_at = lit(&touched_at),
        ))
    }

    fn active_file_by_path(
        &self,
        ctx: &IndexContext,
        relative_path: &Path,
    ) -> Result<Option<(FileId, PathBuf)>> {
        let rows = self.fetch_rows(&format!(
            "MATCH (s:IndexScope {{scope_id: {scope_id}}})-[:CONTAINS_FILE]->(f:File) \
             WHERE f.relative_path = {relative_path} AND f.status = 'indexed' \
             RETURN f.file_id, f.relative_path;",
            scope_id = lit(&ctx.scope_id.0),
            relative_path = lit(&path_key(relative_path)),
        ))?;
        let Some(row) = rows.into_iter().next() else {
            return Ok(None);
        };
        Ok(Some((
            FileId(string_at(&row, 0)?),
            PathBuf::from(string_at(&row, 1)?),
        )))
    }

    fn load_scope_dependencies(&self, ctx: &IndexContext) -> Result<Vec<FileDependency>> {
        let rows = self.fetch_rows(&format!(
            "MATCH (from:File)-[r:IMPORTS_FILE]->(to:File) \
             WHERE from.scope_id = {scope_id} AND to.scope_id = {scope_id} AND from.status = 'indexed' AND to.status = 'indexed' \
             RETURN from.file_id, to.file_id, from.relative_path, to.relative_path, r.raw_target, r.confidence_score, r.confidence_label, r.evidence \
             ORDER BY from.relative_path, to.relative_path;",
            scope_id = lit(&ctx.scope_id.0),
        ))?;
        rows.into_iter().map(file_dependency_from_row).collect()
    }
}

impl GraphStore for KuzuGraphStore {
    fn init_schema(&self) -> Result<()> {
        if self.db_path.exists()
            && self
                .stored_schema_version()
                .is_some_and(|version| version != STORE_SCHEMA_VERSION)
        {
            self.reset_database()?;
        }

        let statements = [
            "CREATE NODE TABLE IF NOT EXISTS SchemaMeta(key STRING PRIMARY KEY, value STRING);",
            "CREATE NODE TABLE IF NOT EXISTS Project(project_id STRING PRIMARY KEY, repo_id STRING, root_path STRING, created_at STRING, updated_at STRING);",
            "CREATE NODE TABLE IF NOT EXISTS Worktree(worktree_id STRING PRIMARY KEY, project_id STRING, root_path STRING, created_at STRING, updated_at STRING);",
            "CREATE NODE TABLE IF NOT EXISTS IndexScope(scope_id STRING PRIMARY KEY, project_id STRING, worktree_id STRING, repo_root STRING, parser_version STRING, graph_schema_version STRING, chunker_version STRING, created_at STRING, updated_at STRING);",
            "CREATE NODE TABLE IF NOT EXISTS File(file_id STRING PRIMARY KEY, repo_id STRING, project_id STRING, worktree_id STRING, scope_id STRING, path STRING, relative_path STRING, absolute_path STRING, language STRING, sha256 STRING, size_bytes INT64, mtime_seconds INT64, status STRING, parser_version STRING, graph_schema_version STRING, chunker_version STRING, last_indexed_run_id STRING, created_at STRING, updated_at STRING, deleted_at STRING);",
            "CREATE NODE TABLE IF NOT EXISTS Symbol(symbol_id STRING PRIMARY KEY, file_id STRING, file_path STRING, name STRING, kind STRING, language STRING, start_line INT64, end_line INT64);",
            "CREATE NODE TABLE IF NOT EXISTS Import(import_id STRING PRIMARY KEY, file_id STRING, raw_target STRING, line INT64);",
            "CREATE NODE TABLE IF NOT EXISTS Chunk(chunk_id STRING PRIMARY KEY, file_id STRING, symbol_id STRING, label STRING, start_line INT64, end_line INT64);",
            "CREATE NODE TABLE IF NOT EXISTS Warning(warning_id STRING PRIMARY KEY, file_id STRING, file_path STRING, stage STRING, message STRING);",
            "CREATE NODE TABLE IF NOT EXISTS IndexRun(run_id STRING PRIMARY KEY, repo_id STRING, project_id STRING, worktree_id STRING, scope_id STRING, repo_root STRING, status STRING, parser_version STRING, graph_schema_version STRING, chunker_version STRING, started_at STRING, finished_at STRING, error STRING, scanned_files INT64, added INT64, modified INT64, unchanged INT64, deleted INT64, skipped INT64, reindex_needed INT64, warnings INT64);",
            "CREATE REL TABLE IF NOT EXISTS HAS_WORKTREE(FROM Project TO Worktree);",
            "CREATE REL TABLE IF NOT EXISTS HAS_SCOPE(FROM Worktree TO IndexScope);",
            "CREATE REL TABLE IF NOT EXISTS CONTAINS_FILE(FROM IndexScope TO File);",
            "CREATE REL TABLE IF NOT EXISTS DEFINES(FROM File TO Symbol);",
            "CREATE REL TABLE IF NOT EXISTS HAS_IMPORT(FROM File TO Import);",
            "CREATE REL TABLE IF NOT EXISTS HAS_CHUNK(FROM File TO Chunk, FROM Symbol TO Chunk);",
            "CREATE REL TABLE IF NOT EXISTS HAS_WARNING(FROM File TO Warning);",
            "CREATE REL TABLE IF NOT EXISTS IMPORTS_FILE(FROM File TO File, raw_target STRING, confidence_score DOUBLE, confidence_label STRING, evidence STRING);",
            "CREATE REL TABLE IF NOT EXISTS TOUCHED_FILE(FROM IndexRun TO File, touched_at STRING);",
        ];
        for statement in statements {
            self.query(statement)?;
        }
        self.query(&format!(
            "MERGE (m:SchemaMeta {{key: 'schema_version'}}) \
             ON CREATE SET m.value = {value} \
             ON MATCH SET m.value = {value};",
            value = lit(STORE_SCHEMA_VERSION),
        ))?;
        Ok(())
    }

    fn begin_index_run(&self, ctx: &IndexContext) -> Result<IndexRun> {
        self.init_schema()?;
        self.ensure_context_nodes(ctx)?;
        let started_at = now_string();
        let run_id = RunId(stable_hash(&[&ctx.scope_id.0, &started_at, "index-run"]));

        self.query(&format!(
            "CREATE (r:IndexRun {{run_id: {run_id}, repo_id: {repo_id}, project_id: {project_id}, worktree_id: {worktree_id}, scope_id: {scope_id}, repo_root: {repo_root}, status: 'running', parser_version: {parser_version}, graph_schema_version: {schema_version}, chunker_version: {chunker_version}, started_at: {started_at}, finished_at: NULL, error: NULL, scanned_files: 0, added: 0, modified: 0, unchanged: 0, deleted: 0, skipped: 0, reindex_needed: 0, warnings: 0}});",
            run_id = lit(&run_id.0),
            repo_id = lit(&ctx.repo_id.0),
            project_id = lit(&ctx.project_id.0),
            worktree_id = lit(&ctx.worktree_id.0),
            scope_id = lit(&ctx.scope_id.0),
            repo_root = lit(&ctx.repo_root.to_string_lossy()),
            parser_version = lit(&ctx.parser_version),
            schema_version = lit(&ctx.schema_version),
            chunker_version = lit(&ctx.chunker_version),
            started_at = lit(&started_at),
        ))?;

        Ok(IndexRun {
            run_id,
            project_id: ctx.project_id.clone(),
            worktree_id: ctx.worktree_id.clone(),
            scope_id: ctx.scope_id.clone(),
            status: IndexRunStatus::Running,
            started_at,
            finished_at: None,
            error: None,
        })
    }

    fn complete_index_run(&self, run_id: &RunId, stats: &IndexStats) -> Result<()> {
        self.query(&format!(
            "MATCH (r:IndexRun {{run_id: {run_id}}}) \
             SET r.status = 'completed', r.finished_at = {finished_at}, r.error = NULL, \
                 r.scanned_files = {scanned_files}, r.added = {added}, r.modified = {modified}, r.unchanged = {unchanged}, r.deleted = {deleted}, r.skipped = {skipped}, r.reindex_needed = {reindex_needed}, r.warnings = {warnings};",
            run_id = lit(&run_id.0),
            finished_at = lit(&now_string()),
            scanned_files = stats.scanned_files,
            added = stats.added,
            modified = stats.modified,
            unchanged = stats.unchanged,
            deleted = stats.deleted,
            skipped = stats.skipped,
            reindex_needed = stats.reindex_needed,
            warnings = stats.warnings,
        ))
    }

    fn fail_index_run(&self, run_id: &RunId, error: &str) -> Result<()> {
        self.query(&format!(
            "MATCH (r:IndexRun {{run_id: {run_id}}}) \
             SET r.status = 'failed', r.finished_at = {finished_at}, r.error = {error};",
            run_id = lit(&run_id.0),
            finished_at = lit(&now_string()),
            error = lit(error),
        ))
    }

    fn load_file_fingerprints(&self, scope_id: &ScopeId) -> Result<Vec<FileFingerprintRecord>> {
        let rows = self.fetch_rows(&format!(
            "MATCH (s:IndexScope {{scope_id: {scope_id}}})-[:CONTAINS_FILE]->(f:File) \
             RETURN f.project_id, f.worktree_id, f.file_id, f.relative_path, f.sha256, f.size_bytes, f.mtime_seconds, f.parser_version, f.graph_schema_version, f.chunker_version, f.last_indexed_run_id, f.created_at, f.updated_at, f.status, f.deleted_at \
             ORDER BY f.relative_path;",
            scope_id = lit(&scope_id.0),
        ))?;

        rows.into_iter()
            .map(|row| {
                Ok(FileFingerprintRecord {
                    project_id: reposcryer_core::ProjectId(string_at(&row, 0)?),
                    worktree_id: reposcryer_core::WorktreeId(string_at(&row, 1)?),
                    scope_id: scope_id.clone(),
                    file_id: FileId(string_at(&row, 2)?),
                    relative_path: PathBuf::from(string_at(&row, 3)?),
                    fingerprint: FileFingerprint {
                        sha256: string_at(&row, 4)?,
                        size_bytes: u64_at(&row, 5)?,
                        mtime_seconds: u64_at(&row, 6)?,
                    },
                    parser_version: string_at(&row, 7)?,
                    schema_version: string_at(&row, 8)?,
                    chunker_version: string_at(&row, 9)?,
                    last_indexed_run_id: optional_string_at(&row, 10)?.map(RunId),
                    created_at: optional_string_at(&row, 11)?,
                    updated_at: optional_string_at(&row, 12)?,
                    status: match string_at(&row, 13)?.as_str() {
                        "deleted" => FileStatus::Deleted,
                        _ => FileStatus::Active,
                    },
                    deleted_at: optional_string_at(&row, 14)?,
                })
            })
            .collect()
    }

    fn build_incremental_plan(
        &self,
        scan: &ScanResult,
        ctx: &IndexContext,
    ) -> Result<IncrementalIndexPlan> {
        self.init_schema()?;
        let stored = self.load_file_fingerprints(&ctx.scope_id)?;
        let stored_by_id: BTreeMap<_, _> = stored
            .iter()
            .map(|record| (record.file_id.0.clone(), record))
            .collect();
        let mut seen = BTreeSet::new();
        let mut changes = Vec::new();

        for file in &scan.files {
            let key = file.file_id.0.clone();
            seen.insert(key.clone());
            let change = match stored_by_id.get(&key) {
                None => FileChange {
                    file_id: file.file_id.clone(),
                    relative_path: file.relative_path.clone(),
                    kind: FileChangeKind::Added,
                    fingerprint: Some(file.fingerprint.clone()),
                    previous_sha256: None,
                    current_sha256: Some(file.fingerprint.sha256.clone()),
                    reason: None,
                },
                Some(record) if record.status == FileStatus::Deleted => FileChange {
                    file_id: file.file_id.clone(),
                    relative_path: file.relative_path.clone(),
                    kind: FileChangeKind::Added,
                    fingerprint: Some(file.fingerprint.clone()),
                    previous_sha256: Some(record.fingerprint.sha256.clone()),
                    current_sha256: Some(file.fingerprint.sha256.clone()),
                    reason: Some("previously soft-deleted".to_string()),
                },
                Some(record) if record.fingerprint.sha256 != file.fingerprint.sha256 => {
                    FileChange {
                        file_id: file.file_id.clone(),
                        relative_path: file.relative_path.clone(),
                        kind: FileChangeKind::Modified,
                        fingerprint: Some(file.fingerprint.clone()),
                        previous_sha256: Some(record.fingerprint.sha256.clone()),
                        current_sha256: Some(file.fingerprint.sha256.clone()),
                        reason: None,
                    }
                }
                Some(record)
                    if record.parser_version != ctx.parser_version
                        || record.schema_version != ctx.schema_version
                        || record.chunker_version != ctx.chunker_version =>
                {
                    FileChange {
                        file_id: file.file_id.clone(),
                        relative_path: file.relative_path.clone(),
                        kind: FileChangeKind::ReindexNeeded,
                        fingerprint: Some(file.fingerprint.clone()),
                        previous_sha256: Some(record.fingerprint.sha256.clone()),
                        current_sha256: Some(file.fingerprint.sha256.clone()),
                        reason: Some("version mismatch".to_string()),
                    }
                }
                Some(_) => FileChange {
                    file_id: file.file_id.clone(),
                    relative_path: file.relative_path.clone(),
                    kind: FileChangeKind::Unchanged,
                    fingerprint: Some(file.fingerprint.clone()),
                    previous_sha256: Some(file.fingerprint.sha256.clone()),
                    current_sha256: Some(file.fingerprint.sha256.clone()),
                    reason: None,
                },
            };
            changes.push(change);
        }

        for record in &stored {
            if record.status == FileStatus::Active && !seen.contains(&record.file_id.0) {
                changes.push(FileChange {
                    file_id: record.file_id.clone(),
                    relative_path: record.relative_path.clone(),
                    kind: FileChangeKind::Deleted,
                    fingerprint: Some(record.fingerprint.clone()),
                    previous_sha256: Some(record.fingerprint.sha256.clone()),
                    current_sha256: None,
                    reason: None,
                });
            }
        }

        for skipped in &scan.skipped_files {
            changes.push(FileChange {
                file_id: file_id_from_relative_path(&skipped.relative_path),
                relative_path: skipped.relative_path.clone(),
                kind: FileChangeKind::Skipped,
                fingerprint: None,
                previous_sha256: None,
                current_sha256: None,
                reason: Some(skipped.reason.clone()),
            });
        }

        changes.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        Ok(IncrementalIndexPlan {
            project_id: Some(ctx.project_id.clone()),
            worktree_id: Some(ctx.worktree_id.clone()),
            scope_id: Some(ctx.scope_id.clone()),
            changes,
        })
    }

    fn replace_file_subgraph(
        &self,
        file: &CodeFile,
        parsed: &ParsedFile,
        run_id: &RunId,
    ) -> Result<()> {
        let (ctx, run_id_value) = self.run_context(run_id)?;
        let now = now_string();
        self.remove_file_subgraph(&file.file_id)?;
        self.query(&format!(
            "MERGE (f:File {{file_id: {file_id}}}) \
             ON CREATE SET f.repo_id = {repo_id}, f.project_id = {project_id}, f.worktree_id = {worktree_id}, f.scope_id = {scope_id}, f.path = {relative_path}, f.relative_path = {relative_path}, f.absolute_path = {absolute_path}, f.language = {language}, f.sha256 = {sha256}, f.size_bytes = {size_bytes}, f.mtime_seconds = {mtime_seconds}, f.status = 'indexed', f.parser_version = {parser_version}, f.graph_schema_version = {schema_version}, f.chunker_version = {chunker_version}, f.last_indexed_run_id = {run_id}, f.created_at = {updated_at}, f.updated_at = {updated_at}, f.deleted_at = NULL \
             ON MATCH SET f.repo_id = {repo_id}, f.project_id = {project_id}, f.worktree_id = {worktree_id}, f.scope_id = {scope_id}, f.path = {relative_path}, f.relative_path = {relative_path}, f.absolute_path = {absolute_path}, f.language = {language}, f.sha256 = {sha256}, f.size_bytes = {size_bytes}, f.mtime_seconds = {mtime_seconds}, f.status = 'indexed', f.parser_version = {parser_version}, f.graph_schema_version = {schema_version}, f.chunker_version = {chunker_version}, f.last_indexed_run_id = {run_id}, f.deleted_at = NULL, f.updated_at = {updated_at};",
            file_id = lit(&file.file_id.0),
            repo_id = lit(&file.repo_id.0),
            project_id = lit(&ctx.project_id.0),
            worktree_id = lit(&ctx.worktree_id.0),
            scope_id = lit(&ctx.scope_id.0),
            relative_path = lit(&file.relative_path.to_string_lossy()),
            absolute_path = lit(&file.path.to_string_lossy()),
            language = lit(file.language.as_str()),
            sha256 = lit(&file.fingerprint.sha256),
            size_bytes = file.fingerprint.size_bytes,
            mtime_seconds = file.fingerprint.mtime_seconds,
            parser_version = lit(&ctx.parser_version),
            schema_version = lit(&ctx.schema_version),
            chunker_version = lit(&ctx.chunker_version),
            run_id = lit(&run_id_value),
            updated_at = lit(&now),
        ))?;
        self.query(&format!(
            "MATCH (s:IndexScope {{scope_id: {scope_id}}}), (f:File {{file_id: {file_id}}}) \
             MERGE (s)-[:CONTAINS_FILE]->(f);",
            scope_id = lit(&ctx.scope_id.0),
            file_id = lit(&file.file_id.0),
        ))?;

        for symbol in &parsed.symbols {
            self.query(&format!(
                "MERGE (s:Symbol {{symbol_id: {symbol_id}}}) \
                 ON CREATE SET s.file_id = {file_id}, s.file_path = {file_path}, s.name = {name}, s.kind = {kind}, s.language = {language}, s.start_line = {start_line}, s.end_line = {end_line} \
                 ON MATCH SET s.file_id = {file_id}, s.file_path = {file_path}, s.name = {name}, s.kind = {kind}, s.language = {language}, s.start_line = {start_line}, s.end_line = {end_line};",
                symbol_id = lit(&symbol.symbol_id.0),
                file_id = lit(&symbol.file_id.0),
                file_path = lit(&symbol.file_path.to_string_lossy()),
                name = lit(&symbol.name),
                kind = lit(&format!("{:?}", symbol.kind)),
                language = lit(symbol.language.as_str()),
                start_line = symbol.range.start_line,
                end_line = symbol.range.end_line,
            ))?;
            self.query(&format!(
                "MATCH (f:File {{file_id: {file_id}}}), (s:Symbol {{symbol_id: {symbol_id}}}) \
                 MERGE (f)-[:DEFINES]->(s);",
                file_id = lit(&file.file_id.0),
                symbol_id = lit(&symbol.symbol_id.0),
            ))?;
        }

        for import in &parsed.imports {
            self.query(&format!(
                "MERGE (i:Import {{import_id: {import_id}}}) \
                 ON CREATE SET i.file_id = {file_id}, i.raw_target = {raw_target}, i.line = {line} \
                 ON MATCH SET i.file_id = {file_id}, i.raw_target = {raw_target}, i.line = {line};",
                import_id = lit(&import.import_id.0),
                file_id = lit(&import.from_file_id.0),
                raw_target = lit(&import.raw_target),
                line = import.line,
            ))?;
            self.query(&format!(
                "MATCH (f:File {{file_id: {file_id}}}), (i:Import {{import_id: {import_id}}}) \
                 MERGE (f)-[:HAS_IMPORT]->(i);",
                file_id = lit(&file.file_id.0),
                import_id = lit(&import.import_id.0),
            ))?;
        }

        for chunk in &parsed.chunks {
            self.query(&format!(
                "MERGE (c:Chunk {{chunk_id: {chunk_id}}}) \
                 ON CREATE SET c.file_id = {file_id}, c.symbol_id = {symbol_id}, c.label = {label}, c.start_line = {start_line}, c.end_line = {end_line} \
                 ON MATCH SET c.file_id = {file_id}, c.symbol_id = {symbol_id}, c.label = {label}, c.start_line = {start_line}, c.end_line = {end_line};",
                chunk_id = lit(&chunk.chunk_id.0),
                file_id = lit(&chunk.file_id.0),
                symbol_id = chunk.symbol_id.as_ref().map(|id| lit(&id.0)).unwrap_or_else(|| "NULL".to_string()),
                label = lit(&chunk.label),
                start_line = chunk.range.start_line,
                end_line = chunk.range.end_line,
            ))?;
            self.query(&format!(
                "MATCH (f:File {{file_id: {file_id}}}), (c:Chunk {{chunk_id: {chunk_id}}}) \
                 MERGE (f)-[:HAS_CHUNK]->(c);",
                file_id = lit(&file.file_id.0),
                chunk_id = lit(&chunk.chunk_id.0),
            ))?;
            if let Some(symbol_id) = &chunk.symbol_id {
                self.query(&format!(
                    "MATCH (s:Symbol {{symbol_id: {symbol_id}}}), (c:Chunk {{chunk_id: {chunk_id}}}) \
                     MERGE (s)-[:HAS_CHUNK]->(c);",
                    symbol_id = lit(&symbol_id.0),
                    chunk_id = lit(&chunk.chunk_id.0),
                ))?;
            }
        }

        for warning in &parsed.warnings {
            self.query(&format!(
                "MERGE (w:Warning {{warning_id: {warning_id}}}) \
                 ON CREATE SET w.file_id = {file_id}, w.file_path = {file_path}, w.stage = {stage}, w.message = {message} \
                 ON MATCH SET w.file_id = {file_id}, w.file_path = {file_path}, w.stage = {stage}, w.message = {message};",
                warning_id = lit(&warning.warning_id.0),
                file_id = warning.file_id.as_ref().map(|id| lit(&id.0)).unwrap_or_else(|| "NULL".to_string()),
                file_path = lit(&warning.file_path.to_string_lossy()),
                stage = lit(&warning.stage),
                message = lit(&warning.message),
            ))?;
            self.query(&format!(
                "MATCH (f:File {{file_id: {file_id}}}), (w:Warning {{warning_id: {warning_id}}}) \
                 MERGE (f)-[:HAS_WARNING]->(w);",
                file_id = lit(&file.file_id.0),
                warning_id = lit(&warning.warning_id.0),
            ))?;
        }

        self.touch_file(&file.file_id, run_id)
    }

    fn mark_file_deleted(&self, file_id: &FileId, run_id: &RunId) -> Result<()> {
        self.remove_file_subgraph(file_id)?;
        self.remove_file_dependency_edges(file_id, true)?;
        self.query(&format!(
            "MATCH (f:File {{file_id: {file_id}}}) \
             SET f.status = 'deleted', f.deleted_at = {deleted_at}, f.updated_at = {deleted_at}, f.last_indexed_run_id = {run_id};",
            file_id = lit(&file_id.0),
            deleted_at = lit(&now_string()),
            run_id = lit(&run_id.0),
        ))?;
        self.touch_file(file_id, run_id)
    }

    fn repo_status(&self, scan: &ScanResult, ctx: &IndexContext) -> Result<RepoStatus> {
        let plan = self.build_incremental_plan(scan, ctx)?;
        let stats = plan.stats();
        let tracked_files = self
            .load_file_fingerprints(&ctx.scope_id)?
            .into_iter()
            .filter(|record| record.status == FileStatus::Active)
            .count();
        Ok(RepoStatus {
            tracked_files,
            scan_files: scan.files.len(),
            added: stats.added,
            modified: stats.modified,
            unchanged: stats.unchanged,
            deleted: stats.deleted,
            skipped: stats.skipped,
            reindex_needed: stats.reindex_needed,
        })
    }

    fn changed_files(&self, scan: &ScanResult, ctx: &IndexContext) -> Result<Vec<FileChange>> {
        Ok(self
            .build_incremental_plan(scan, ctx)?
            .changes
            .into_iter()
            .filter(|change| change.kind != FileChangeKind::Unchanged)
            .collect())
    }

    fn rebuild_scope_import_edges(&self, ctx: &IndexContext) -> Result<usize> {
        self.init_schema()?;
        self.query(&format!(
            "MATCH (from:File)-[r:IMPORTS_FILE]->(:File) \
             WHERE from.scope_id = {scope_id} DELETE r;",
            scope_id = lit(&ctx.scope_id.0),
        ))?;

        let file_rows = self.fetch_rows(&format!(
            "MATCH (s:IndexScope {{scope_id: {scope_id}}})-[:CONTAINS_FILE]->(f:File) \
             WHERE f.status = 'indexed' \
             RETURN f.file_id, f.relative_path, f.language ORDER BY f.relative_path;",
            scope_id = lit(&ctx.scope_id.0),
        ))?;
        let files: Vec<StoredFile> = file_rows
            .into_iter()
            .map(|row| {
                Ok(StoredFile {
                    file_id: FileId(string_at(&row, 0)?),
                    relative_path: PathBuf::from(string_at(&row, 1)?),
                    language: language_from_store(&string_at(&row, 2)?),
                })
            })
            .collect::<Result<_>>()?;
        let files_by_path: BTreeMap<_, _> = files
            .iter()
            .map(|file| (path_key(&file.relative_path), file))
            .collect();

        let import_rows = self.fetch_rows(&format!(
            "MATCH (s:IndexScope {{scope_id: {scope_id}}})-[:CONTAINS_FILE]->(f:File)-[:HAS_IMPORT]->(i:Import) \
             WHERE f.status = 'indexed' \
             RETURN f.file_id, f.relative_path, f.language, i.raw_target ORDER BY f.relative_path, i.line;",
            scope_id = lit(&ctx.scope_id.0),
        ))?;

        let mut created = BTreeSet::new();
        for row in import_rows {
            let from = StoredFile {
                file_id: FileId(string_at(&row, 0)?),
                relative_path: PathBuf::from(string_at(&row, 1)?),
                language: language_from_store(&string_at(&row, 2)?),
            };
            let raw_target = string_at(&row, 3)?;
            let Some(to) = resolve_local_file_import(&from, &raw_target, &files_by_path) else {
                continue;
            };
            let key = format!("{}:{}", from.file_id.0, to.file_id.0);
            if !created.insert(key) {
                continue;
            }
            self.query(&format!(
                "MATCH (from:File {{file_id: {from_id}}}), (to:File {{file_id: {to_id}}}) \
                 MERGE (from)-[r:IMPORTS_FILE]->(to) \
                 ON CREATE SET r.raw_target = {raw_target}, r.confidence_score = 1.0, r.confidence_label = 'high', r.evidence = 'rust module path resolved to local file' \
                 ON MATCH SET r.raw_target = {raw_target}, r.confidence_score = 1.0, r.confidence_label = 'high', r.evidence = 'rust module path resolved to local file';",
                from_id = lit(&from.file_id.0),
                to_id = lit(&to.file_id.0),
                raw_target = lit(&raw_target),
            ))?;
        }

        Ok(created.len())
    }

    fn explain_file(
        &self,
        ctx: &IndexContext,
        relative_path: &Path,
    ) -> Result<Option<FileExplanation>> {
        self.init_schema()?;
        let file_path = path_key(relative_path);
        let rows = self.fetch_rows(&format!(
            "MATCH (s:IndexScope {{scope_id: {scope_id}}})-[:CONTAINS_FILE]->(f:File) \
             WHERE f.relative_path = {relative_path} AND f.status = 'indexed' \
             RETURN f.file_id, f.relative_path, f.language;",
            scope_id = lit(&ctx.scope_id.0),
            relative_path = lit(&file_path),
        ))?;
        let Some(row) = rows.into_iter().next() else {
            return Ok(None);
        };
        let file_id = FileId(string_at(&row, 0)?);
        let relative_path = PathBuf::from(string_at(&row, 1)?);
        let language = language_from_store(&string_at(&row, 2)?);

        let symbol_rows = self.fetch_rows(&format!(
            "MATCH (f:File {{file_id: {file_id}}})-[:DEFINES]->(s:Symbol) \
             RETURN s.symbol_id, s.file_id, s.file_path, s.name, s.kind, s.language, s.start_line, s.end_line ORDER BY s.start_line, s.name;",
            file_id = lit(&file_id.0),
        ))?;
        let symbols = symbol_rows
            .into_iter()
            .map(symbol_from_row)
            .collect::<Result<Vec<_>>>()?;

        let import_rows = self.fetch_rows(&format!(
            "MATCH (f:File {{file_id: {file_id}}})-[:HAS_IMPORT]->(i:Import) \
             RETURN i.import_id, i.file_id, i.raw_target, i.line ORDER BY i.line, i.raw_target;",
            file_id = lit(&file_id.0),
        ))?;
        let imports = import_rows
            .into_iter()
            .map(import_from_row)
            .collect::<Result<Vec<_>>>()?;

        let dependency_rows = self.fetch_rows(&format!(
            "MATCH (from:File {{file_id: {file_id}}})-[r:IMPORTS_FILE]->(to:File) \
             RETURN from.file_id, to.file_id, from.relative_path, to.relative_path, r.raw_target, r.confidence_score, r.confidence_label, r.evidence ORDER BY to.relative_path;",
            file_id = lit(&file_id.0),
        ))?;
        let dependencies = dependency_rows
            .into_iter()
            .map(file_dependency_from_row)
            .collect::<Result<Vec<_>>>()?;

        let warning_rows = self.fetch_rows(&format!(
            "MATCH (f:File {{file_id: {file_id}}})-[:HAS_WARNING]->(w:Warning) \
             RETURN w.warning_id, w.file_id, w.file_path, w.stage, w.message ORDER BY w.stage, w.message;",
            file_id = lit(&file_id.0),
        ))?;
        let warnings = warning_rows
            .into_iter()
            .map(warning_from_row)
            .collect::<Result<Vec<_>>>()?;

        Ok(Some(FileExplanation {
            file_id,
            relative_path,
            language,
            symbols,
            imports,
            dependencies,
            warnings,
        }))
    }

    fn file_neighbors(
        &self,
        ctx: &IndexContext,
        relative_path: &Path,
    ) -> Result<Option<FileNeighborhood>> {
        self.init_schema()?;
        let Some((file_id, relative_path)) = self.active_file_by_path(ctx, relative_path)? else {
            return Ok(None);
        };
        let dependencies = self.load_scope_dependencies(ctx)?;
        let outgoing = dependencies
            .iter()
            .filter(|dependency| dependency.from_file_id == file_id)
            .cloned()
            .collect();
        let incoming = dependencies
            .into_iter()
            .filter(|dependency| dependency.to_file_id == file_id)
            .collect();

        Ok(Some(FileNeighborhood {
            file_id,
            relative_path,
            outgoing,
            incoming,
        }))
    }

    fn file_impact(&self, ctx: &IndexContext, relative_path: &Path) -> Result<Option<FileImpact>> {
        self.init_schema()?;
        let Some((file_id, relative_path)) = self.active_file_by_path(ctx, relative_path)? else {
            return Ok(None);
        };
        let dependencies = self.load_scope_dependencies(ctx)?;
        let mut incoming_by_target: BTreeMap<String, Vec<&FileDependency>> = BTreeMap::new();
        for dependency in &dependencies {
            incoming_by_target
                .entry(dependency.to_file_id.0.clone())
                .or_default()
                .push(dependency);
        }

        let mut seen = BTreeSet::new();
        let mut queue = VecDeque::from([(file_id.0.clone(), relative_path.clone(), 0_usize)]);
        let mut impacted_files = Vec::new();

        while let Some((current_id, current_path, depth)) = queue.pop_front() {
            let Some(incoming) = incoming_by_target.get(&current_id) else {
                continue;
            };
            for dependency in incoming {
                if !seen.insert(dependency.from_file_id.0.clone()) {
                    continue;
                }
                let impacted = ImpactedFile {
                    file_id: dependency.from_file_id.clone(),
                    relative_path: dependency.from_relative_path.clone(),
                    depth: depth + 1,
                    via_relative_path: current_path.clone(),
                };
                queue.push_back((
                    dependency.from_file_id.0.clone(),
                    dependency.from_relative_path.clone(),
                    depth + 1,
                ));
                impacted_files.push(impacted);
            }
        }

        impacted_files.sort_by(|left, right| {
            left.depth
                .cmp(&right.depth)
                .then_with(|| left.relative_path.cmp(&right.relative_path))
        });

        Ok(Some(FileImpact {
            file_id,
            relative_path,
            impacted_files,
        }))
    }
}

#[derive(Debug)]
struct StoredFile {
    file_id: FileId,
    relative_path: PathBuf,
    language: Language,
}

fn resolve_local_file_import<'a>(
    from: &StoredFile,
    raw_target: &str,
    files_by_path: &'a BTreeMap<String, &'a StoredFile>,
) -> Option<&'a StoredFile> {
    if from.language != Language::Rust {
        return None;
    }

    let mut target = raw_target.trim();
    for prefix in ["crate::", "self::", "super::"] {
        if let Some(stripped) = target.strip_prefix(prefix) {
            target = stripped;
            break;
        }
    }
    let first_segment = target
        .split("::")
        .next()
        .unwrap_or_default()
        .trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_');
    if first_segment.is_empty() || matches!(first_segment, "std" | "core" | "alloc") {
        return None;
    }

    rust_module_candidates(&from.relative_path, first_segment)
        .into_iter()
        .find_map(|candidate| files_by_path.get(&path_key(&candidate)).copied())
}

fn rust_module_candidates(from_path: &Path, module: &str) -> Vec<PathBuf> {
    let parent = from_path.parent().unwrap_or_else(|| Path::new(""));
    let stem = from_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let mut candidates = Vec::new();

    if !matches!(stem, "main" | "lib" | "mod" | "") {
        candidates.push(parent.join(stem).join(format!("{module}.rs")));
        candidates.push(parent.join(stem).join(module).join("mod.rs"));
    }
    candidates.push(parent.join(format!("{module}.rs")));
    candidates.push(parent.join(module).join("mod.rs"));
    candidates.push(PathBuf::from("src").join(format!("{module}.rs")));
    candidates.push(PathBuf::from("src").join(module).join("mod.rs"));
    candidates
}

fn path_key(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn language_from_store(value: &str) -> Language {
    match value {
        "rust" => Language::Rust,
        "python" => Language::Python,
        "javascript" => Language::JavaScript,
        "typescript" => Language::TypeScript,
        "java" => Language::Java,
        "go" => Language::Go,
        _ => Language::Unknown,
    }
}

fn symbol_kind_from_store(value: &str) -> SymbolKind {
    match value {
        "Struct" => SymbolKind::Struct,
        "Enum" => SymbolKind::Enum,
        "Trait" => SymbolKind::Trait,
        "Function" => SymbolKind::Function,
        "Method" => SymbolKind::Method,
        "Class" => SymbolKind::Class,
        "Interface" => SymbolKind::Interface,
        "Module" => SymbolKind::Module,
        _ => SymbolKind::Type,
    }
}

fn symbol_from_row(row: Vec<Value>) -> Result<Symbol> {
    Ok(Symbol {
        symbol_id: SymbolId(string_at(&row, 0)?),
        file_id: FileId(string_at(&row, 1)?),
        file_path: PathBuf::from(string_at(&row, 2)?),
        name: string_at(&row, 3)?,
        kind: symbol_kind_from_store(&string_at(&row, 4)?),
        language: language_from_store(&string_at(&row, 5)?),
        range: CodeChunk {
            start_line: u64_at(&row, 6)?.try_into()?,
            end_line: u64_at(&row, 7)?.try_into()?,
        },
    })
}

fn import_from_row(row: Vec<Value>) -> Result<Import> {
    Ok(Import {
        import_id: ImportId(string_at(&row, 0)?),
        from_file_id: FileId(string_at(&row, 1)?),
        raw_target: string_at(&row, 2)?,
        line: u64_at(&row, 3)?.try_into()?,
    })
}

fn file_dependency_from_row(row: Vec<Value>) -> Result<FileDependency> {
    Ok(FileDependency {
        from_file_id: FileId(string_at(&row, 0)?),
        to_file_id: FileId(string_at(&row, 1)?),
        from_relative_path: PathBuf::from(string_at(&row, 2)?),
        to_relative_path: PathBuf::from(string_at(&row, 3)?),
        raw_target: string_at(&row, 4)?,
        confidence: Confidence {
            score: f32_at(&row, 5)?,
            label: string_at(&row, 6)?,
        },
        evidence: Evidence {
            source: "kuzu-import-resolver".to_string(),
            detail: string_at(&row, 7)?,
        },
    })
}

fn warning_from_row(row: Vec<Value>) -> Result<IndexWarning> {
    Ok(IndexWarning {
        warning_id: WarningId(string_at(&row, 0)?),
        file_id: optional_string_at(&row, 1)?.map(FileId),
        file_path: PathBuf::from(string_at(&row, 2)?),
        stage: string_at(&row, 3)?,
        message: string_at(&row, 4)?,
    })
}

fn now_string() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

fn lit(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}

fn string_at(row: &[Value], idx: usize) -> Result<String> {
    match row.get(idx) {
        Some(Value::String(value)) => Ok(value.clone()),
        Some(Value::Int64(value)) => Ok(value.to_string()),
        Some(Value::UInt64(value)) => Ok(value.to_string()),
        Some(Value::Int32(value)) => Ok(value.to_string()),
        Some(Value::UInt32(value)) => Ok(value.to_string()),
        Some(other) => bail!("expected string-like value at column {idx}, got {other:?}"),
        None => bail!("missing column {idx}"),
    }
}

fn optional_string_at(row: &[Value], idx: usize) -> Result<Option<String>> {
    match row.get(idx) {
        Some(Value::Null(_)) => Ok(None),
        Some(_) => Ok(Some(string_at(row, idx)?)),
        None => bail!("missing column {idx}"),
    }
}

fn u64_at(row: &[Value], idx: usize) -> Result<u64> {
    match row.get(idx) {
        Some(Value::UInt64(value)) => Ok(*value),
        Some(Value::Int64(value)) => Ok((*value).try_into()?),
        Some(Value::UInt32(value)) => Ok((*value).into()),
        Some(Value::Int32(value)) => Ok((*value).try_into()?),
        Some(other) => bail!("expected integer value at column {idx}, got {other:?}"),
        None => bail!("missing column {idx}"),
    }
}

fn f32_at(row: &[Value], idx: usize) -> Result<f32> {
    match row.get(idx) {
        Some(Value::Float(value)) => Ok(*value),
        Some(Value::Double(value)) => Ok(*value as f32),
        Some(Value::Int64(value)) => Ok(*value as f32),
        Some(Value::Int32(value)) => Ok(*value as f32),
        Some(other) => bail!("expected float value at column {idx}, got {other:?}"),
        None => bail!("missing column {idx}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reposcryer_core::{
        Chunk, ChunkId, CodeChunk, FileFingerprint, FileId, Import, ImportId, IndexWarning,
        Language, ProjectId, RepoId, ScopeId, Symbol, SymbolId, SymbolKind, WarningId, WorktreeId,
    };
    use std::path::{Path, PathBuf};
    use tempfile::tempdir;

    fn test_store() -> (tempfile::TempDir, KuzuGraphStore) {
        let dir = tempdir().expect("tempdir");
        let store = KuzuGraphStore::new(dir.path().join("db"));
        (dir, store)
    }

    fn sample_context(root: &Path, parser_version: &str) -> IndexContext {
        IndexContext {
            repo_id: RepoId("repo".to_string()),
            project_id: ProjectId("project".to_string()),
            worktree_id: WorktreeId("worktree".to_string()),
            scope_id: ScopeId("scope".to_string()),
            repo_root: root.to_path_buf(),
            parser_version: parser_version.to_string(),
            schema_version: STORE_SCHEMA_VERSION.to_string(),
            chunker_version: "symbol-range-v1".to_string(),
        }
    }

    fn sample_file(name: &str, sha: &str) -> CodeFile {
        let relative_path = PathBuf::from(name);
        CodeFile {
            repo_id: RepoId("repo".to_string()),
            file_id: FileId(stable_hash(&[name])),
            path: PathBuf::from(format!("/tmp/repo/{name}")),
            relative_path,
            language: Language::Rust,
            fingerprint: FileFingerprint {
                sha256: sha.to_string(),
                size_bytes: 10,
                mtime_seconds: 1,
            },
        }
    }

    fn parsed_for(file: &CodeFile, symbol_name: &str) -> ParsedFile {
        let symbol_id = SymbolId(stable_hash(&[&file.file_id.0, symbol_name]));
        ParsedFile {
            file_id: file.file_id.clone(),
            symbols: vec![Symbol {
                symbol_id: symbol_id.clone(),
                file_id: file.file_id.clone(),
                file_path: file.relative_path.clone(),
                name: symbol_name.to_string(),
                kind: SymbolKind::Function,
                range: CodeChunk {
                    start_line: 1,
                    end_line: 3,
                },
                language: file.language.clone(),
            }],
            imports: vec![Import {
                import_id: ImportId(stable_hash(&[&file.file_id.0, "crate::dep"])),
                from_file_id: file.file_id.clone(),
                raw_target: "crate::dep".to_string(),
                line: 1,
            }],
            chunks: vec![Chunk {
                chunk_id: ChunkId(stable_hash(&[&file.file_id.0, "chunk"])),
                file_id: file.file_id.clone(),
                symbol_id: Some(symbol_id),
                label: "symbol-body".to_string(),
                range: CodeChunk {
                    start_line: 1,
                    end_line: 3,
                },
            }],
            warnings: vec![IndexWarning {
                warning_id: WarningId(stable_hash(&[&file.file_id.0, "warning"])),
                file_id: Some(file.file_id.clone()),
                file_path: file.relative_path.clone(),
                stage: "parse".to_string(),
                message: "heuristic parser".to_string(),
            }],
        }
    }

    fn query_count(store: &KuzuGraphStore, query: &str) -> u64 {
        let rows = store.fetch_rows(query).expect("query succeeds");
        let row = rows.into_iter().next().expect("count row");
        u64_at(&row, 0).expect("count value")
    }

    #[test]
    fn init_schema_creates_required_tables() {
        let (_dir, store) = test_store();
        store.init_schema().expect("schema init");

        let rows = store
            .fetch_rows("CALL show_tables() RETURN name ORDER BY name;")
            .expect("show tables");
        let names: Vec<_> = rows
            .into_iter()
            .map(|row| string_at(&row, 0).expect("table name"))
            .collect();

        for name in [
            "Chunk",
            "File",
            "HAS_CHUNK",
            "HAS_IMPORT",
            "HAS_SCOPE",
            "HAS_WARNING",
            "HAS_WORKTREE",
            "Import",
            "IMPORTS_FILE",
            "IndexRun",
            "IndexScope",
            "Project",
            "Symbol",
            "TOUCHED_FILE",
            "Warning",
            "Worktree",
        ] {
            assert!(
                names.iter().any(|candidate| candidate == name),
                "missing {name}"
            );
        }
    }

    #[test]
    fn first_index_creates_file_symbol_and_edges() {
        let (dir, store) = test_store();
        let ctx = sample_context(dir.path(), "parser-v1");
        let run = store.begin_index_run(&ctx).expect("begin run");
        let file = sample_file("src/main.rs", "sha-a");
        let parsed = parsed_for(&file, "run");

        store
            .replace_file_subgraph(&file, &parsed, &run.run_id)
            .expect("replace file");
        let stats = IndexStats {
            scanned_files: 1,
            added: 1,
            ..IndexStats::default()
        };
        store
            .complete_index_run(&run.run_id, &stats)
            .expect("complete run");

        assert_eq!(query_count(&store, "MATCH (f:File) RETURN COUNT(*)"), 1);
        assert_eq!(query_count(&store, "MATCH (s:Symbol) RETURN COUNT(*)"), 1);
        assert_eq!(
            query_count(
                &store,
                "MATCH (:File)-[:DEFINES]->(:Symbol) RETURN COUNT(*)"
            ),
            1
        );
        assert_eq!(
            query_count(
                &store,
                "MATCH (:IndexRun)-[:TOUCHED_FILE]->(:File) RETURN COUNT(*)"
            ),
            1
        );

        let rows = store
            .fetch_rows(
                "MATCH (f:File) RETURN f.project_id, f.worktree_id, f.scope_id, f.mtime_seconds, f.last_indexed_run_id;",
            )
            .expect("file metadata");
        assert_eq!(
            string_at(&rows[0], 0).expect("project id"),
            ctx.project_id.0
        );
        assert_eq!(
            string_at(&rows[0], 1).expect("worktree id"),
            ctx.worktree_id.0
        );
        assert_eq!(string_at(&rows[0], 2).expect("scope id"), ctx.scope_id.0);
        assert_eq!(u64_at(&rows[0], 3).expect("mtime"), 1);
        assert_eq!(string_at(&rows[0], 4).expect("last run"), run.run_id.0);
    }

    #[test]
    fn second_index_without_changes_reports_unchanged() {
        let (dir, store) = test_store();
        let ctx = sample_context(dir.path(), "parser-v1");
        let run = store.begin_index_run(&ctx).expect("begin");
        let file = sample_file("src/main.rs", "sha-a");
        let parsed = parsed_for(&file, "run");
        store
            .replace_file_subgraph(&file, &parsed, &run.run_id)
            .expect("index file");
        store
            .complete_index_run(
                &run.run_id,
                &IndexStats {
                    scanned_files: 1,
                    added: 1,
                    ..IndexStats::default()
                },
            )
            .expect("complete");

        let scan = ScanResult {
            repo_id: RepoId("repo".to_string()),
            repo_root: dir.path().to_path_buf(),
            files: vec![file],
            skipped_files: Vec::new(),
        };
        let plan = store.build_incremental_plan(&scan, &ctx).expect("plan");

        assert_eq!(plan.changes.len(), 1);
        assert_eq!(plan.changes[0].kind, FileChangeKind::Unchanged);
    }

    #[test]
    fn modified_file_replaces_only_its_subgraph() {
        let (dir, store) = test_store();
        let ctx = sample_context(dir.path(), "parser-v1");
        let run = store.begin_index_run(&ctx).expect("begin");
        let file_a = sample_file("src/a.rs", "sha-a");
        let file_b = sample_file("src/b.rs", "sha-b");
        store
            .replace_file_subgraph(&file_a, &parsed_for(&file_a, "old_a"), &run.run_id)
            .expect("index a");
        store
            .replace_file_subgraph(&file_b, &parsed_for(&file_b, "old_b"), &run.run_id)
            .expect("index b");
        store
            .replace_file_subgraph(&file_b, &parsed_for(&file_b, "new_b"), &run.run_id)
            .expect("replace b");

        let rows = store
            .fetch_rows("MATCH (s:Symbol) RETURN s.name ORDER BY s.name;")
            .expect("load symbols");
        let names: Vec<_> = rows
            .into_iter()
            .map(|row| string_at(&row, 0).expect("symbol name"))
            .collect();

        assert!(names.iter().any(|name| name == "old_a"));
        assert!(names.iter().any(|name| name == "new_b"));
        assert!(!names.iter().any(|name| name == "old_b"));
    }

    #[test]
    fn deleted_file_is_soft_deleted() {
        let (dir, store) = test_store();
        let ctx = sample_context(dir.path(), "parser-v1");
        let run = store.begin_index_run(&ctx).expect("begin");
        let file = sample_file("src/main.rs", "sha-a");
        store
            .replace_file_subgraph(&file, &parsed_for(&file, "run"), &run.run_id)
            .expect("index");
        store
            .mark_file_deleted(&file.file_id, &run.run_id)
            .expect("soft delete");

        let rows = store
            .fetch_rows("MATCH (f:File) RETURN f.status, f.deleted_at;")
            .expect("file row");
        assert_eq!(string_at(&rows[0], 0).expect("status"), "deleted");
        assert!(
            optional_string_at(&rows[0], 1)
                .expect("deleted_at")
                .is_some()
        );
        assert_eq!(
            query_count(
                &store,
                "MATCH (:File)-[:DEFINES]->(:Symbol) RETURN COUNT(*)"
            ),
            0
        );
    }

    #[test]
    fn parser_version_change_triggers_reindex_needed() {
        let (dir, store) = test_store();
        let ctx_v1 = sample_context(dir.path(), "parser-v1");
        let run = store.begin_index_run(&ctx_v1).expect("begin");
        let file = sample_file("src/main.rs", "sha-a");
        store
            .replace_file_subgraph(&file, &parsed_for(&file, "run"), &run.run_id)
            .expect("index");
        store
            .complete_index_run(
                &run.run_id,
                &IndexStats {
                    scanned_files: 1,
                    added: 1,
                    ..IndexStats::default()
                },
            )
            .expect("complete");

        let ctx_v2 = sample_context(dir.path(), "parser-v2");
        let scan = ScanResult {
            repo_id: RepoId("repo".to_string()),
            repo_root: dir.path().to_path_buf(),
            files: vec![file],
            skipped_files: Vec::new(),
        };
        let plan = store.build_incremental_plan(&scan, &ctx_v2).expect("plan");

        assert_eq!(plan.changes[0].kind, FileChangeKind::ReindexNeeded);
    }

    #[test]
    fn failed_index_run_is_recorded() {
        let (dir, store) = test_store();
        let ctx = sample_context(dir.path(), "parser-v1");
        let run = store.begin_index_run(&ctx).expect("begin");

        store
            .fail_index_run(&run.run_id, "boom")
            .expect("record failure");

        let rows = store
            .fetch_rows("MATCH (r:IndexRun) RETURN r.status, r.error;")
            .expect("runs");
        assert_eq!(string_at(&rows[0], 0).expect("status"), "failed");
        assert_eq!(string_at(&rows[0], 1).expect("error"), "boom");
    }

    #[test]
    fn repeated_replace_file_subgraph_is_idempotent() {
        let (dir, store) = test_store();
        let ctx = sample_context(dir.path(), "parser-v1");
        let run = store.begin_index_run(&ctx).expect("begin");
        let file = sample_file("src/main.rs", "sha-a");
        let parsed = parsed_for(&file, "run");

        store
            .replace_file_subgraph(&file, &parsed, &run.run_id)
            .expect("replace 1");
        store
            .replace_file_subgraph(&file, &parsed, &run.run_id)
            .expect("replace 2");

        assert_eq!(query_count(&store, "MATCH (s:Symbol) RETURN COUNT(*)"), 1);
        assert_eq!(query_count(&store, "MATCH (i:Import) RETURN COUNT(*)"), 1);
        assert_eq!(query_count(&store, "MATCH (c:Chunk) RETURN COUNT(*)"), 1);
        assert_eq!(query_count(&store, "MATCH (w:Warning) RETURN COUNT(*)"), 1);
    }

    #[test]
    fn rebuild_scope_import_edges_resolves_rust_mod_imports() {
        let (dir, store) = test_store();
        let ctx = sample_context(dir.path(), "parser-v1");
        let run = store.begin_index_run(&ctx).expect("begin");
        let main = sample_file("src/main.rs", "sha-main");
        let auth = sample_file("src/auth.rs", "sha-auth");
        let mut parsed_main = parsed_for(&main, "main");
        parsed_main.imports = vec![Import {
            import_id: ImportId(stable_hash(&[&main.file_id.0, "auth"])),
            from_file_id: main.file_id.clone(),
            raw_target: "auth".to_string(),
            line: 1,
        }];

        store
            .replace_file_subgraph(&main, &parsed_main, &run.run_id)
            .expect("index main");
        store
            .replace_file_subgraph(&auth, &parsed_for(&auth, "AuthService"), &run.run_id)
            .expect("index auth");

        let rebuilt = store
            .rebuild_scope_import_edges(&ctx)
            .expect("rebuild import edges");

        assert_eq!(rebuilt, 1);
        assert_eq!(
            query_count(
                &store,
                "MATCH (:File)-[:IMPORTS_FILE]->(:File) RETURN COUNT(*)"
            ),
            1
        );
        let rows = store
            .fetch_rows("MATCH (from:File)-[r:IMPORTS_FILE]->(to:File) RETURN from.relative_path, to.relative_path, r.raw_target, r.evidence;")
            .expect("dependency row");
        assert_eq!(string_at(&rows[0], 0).expect("from path"), "src/main.rs");
        assert_eq!(string_at(&rows[0], 1).expect("to path"), "src/auth.rs");
        assert_eq!(string_at(&rows[0], 2).expect("raw target"), "auth");
        assert_eq!(
            string_at(&rows[0], 3).expect("evidence"),
            "rust module path resolved to local file"
        );
    }

    #[test]
    fn rebuild_scope_import_edges_is_idempotent() {
        let (dir, store) = test_store();
        let ctx = sample_context(dir.path(), "parser-v1");
        let run = store.begin_index_run(&ctx).expect("begin");
        let main = sample_file("src/main.rs", "sha-main");
        let auth = sample_file("src/auth.rs", "sha-auth");
        let mut parsed_main = parsed_for(&main, "main");
        parsed_main.imports = vec![Import {
            import_id: ImportId(stable_hash(&[&main.file_id.0, "auth"])),
            from_file_id: main.file_id.clone(),
            raw_target: "auth".to_string(),
            line: 1,
        }];
        store
            .replace_file_subgraph(&main, &parsed_main, &run.run_id)
            .expect("index main");
        store
            .replace_file_subgraph(&auth, &parsed_for(&auth, "AuthService"), &run.run_id)
            .expect("index auth");

        store
            .rebuild_scope_import_edges(&ctx)
            .expect("first rebuild");
        store
            .rebuild_scope_import_edges(&ctx)
            .expect("second rebuild");

        assert_eq!(
            query_count(
                &store,
                "MATCH (:File)-[:IMPORTS_FILE]->(:File) RETURN COUNT(*)"
            ),
            1
        );
    }

    #[test]
    fn explain_file_returns_symbols_imports_and_file_dependencies() {
        let (dir, store) = test_store();
        let ctx = sample_context(dir.path(), "parser-v1");
        let run = store.begin_index_run(&ctx).expect("begin");
        let main = sample_file("src/main.rs", "sha-main");
        let auth = sample_file("src/auth.rs", "sha-auth");
        let mut parsed_main = parsed_for(&main, "main");
        parsed_main.imports = vec![Import {
            import_id: ImportId(stable_hash(&[&main.file_id.0, "auth"])),
            from_file_id: main.file_id.clone(),
            raw_target: "auth".to_string(),
            line: 1,
        }];
        store
            .replace_file_subgraph(&main, &parsed_main, &run.run_id)
            .expect("index main");
        store
            .replace_file_subgraph(&auth, &parsed_for(&auth, "AuthService"), &run.run_id)
            .expect("index auth");
        store
            .rebuild_scope_import_edges(&ctx)
            .expect("rebuild dependencies");

        let explanation = store
            .explain_file(&ctx, Path::new("src/main.rs"))
            .expect("explain file")
            .expect("file exists");

        assert_eq!(explanation.relative_path, PathBuf::from("src/main.rs"));
        assert!(
            explanation
                .symbols
                .iter()
                .any(|symbol| symbol.name == "main")
        );
        assert!(
            explanation
                .imports
                .iter()
                .any(|import| import.raw_target == "auth")
        );
        assert!(
            explanation
                .dependencies
                .iter()
                .any(|dependency| dependency.to_relative_path == Path::new("src/auth.rs"))
        );
    }

    #[test]
    fn file_neighbors_returns_incoming_and_outgoing_dependencies() {
        let (dir, store) = test_store();
        let ctx = sample_context(dir.path(), "parser-v1");
        let run = store.begin_index_run(&ctx).expect("begin");
        let main = sample_file("src/main.rs", "sha-main");
        let auth = sample_file("src/auth.rs", "sha-auth");
        let app = sample_file("src/app.rs", "sha-app");
        let mut parsed_main = parsed_for(&main, "main");
        parsed_main.imports = vec![Import {
            import_id: ImportId(stable_hash(&[&main.file_id.0, "auth"])),
            from_file_id: main.file_id.clone(),
            raw_target: "auth".to_string(),
            line: 1,
        }];
        let mut parsed_app = parsed_for(&app, "app");
        parsed_app.imports = vec![Import {
            import_id: ImportId(stable_hash(&[&app.file_id.0, "main"])),
            from_file_id: app.file_id.clone(),
            raw_target: "main".to_string(),
            line: 1,
        }];

        store
            .replace_file_subgraph(&main, &parsed_main, &run.run_id)
            .expect("index main");
        store
            .replace_file_subgraph(&auth, &parsed_for(&auth, "AuthService"), &run.run_id)
            .expect("index auth");
        store
            .replace_file_subgraph(&app, &parsed_app, &run.run_id)
            .expect("index app");
        store
            .rebuild_scope_import_edges(&ctx)
            .expect("rebuild dependencies");

        let neighbors = store
            .file_neighbors(&ctx, Path::new("src/main.rs"))
            .expect("neighbors")
            .expect("file exists");

        assert!(
            neighbors
                .outgoing
                .iter()
                .any(|dependency| dependency.to_relative_path == Path::new("src/auth.rs"))
        );
        assert!(
            neighbors
                .incoming
                .iter()
                .any(|dependency| dependency.from_relative_path == Path::new("src/app.rs"))
        );
    }

    #[test]
    fn file_impact_returns_transitive_reverse_dependencies() {
        let (dir, store) = test_store();
        let ctx = sample_context(dir.path(), "parser-v1");
        let run = store.begin_index_run(&ctx).expect("begin");
        let auth = sample_file("src/auth.rs", "sha-auth");
        let main = sample_file("src/main.rs", "sha-main");
        let app = sample_file("src/app.rs", "sha-app");
        let mut parsed_main = parsed_for(&main, "main");
        parsed_main.imports = vec![Import {
            import_id: ImportId(stable_hash(&[&main.file_id.0, "auth"])),
            from_file_id: main.file_id.clone(),
            raw_target: "auth".to_string(),
            line: 1,
        }];
        let mut parsed_app = parsed_for(&app, "app");
        parsed_app.imports = vec![Import {
            import_id: ImportId(stable_hash(&[&app.file_id.0, "main"])),
            from_file_id: app.file_id.clone(),
            raw_target: "main".to_string(),
            line: 1,
        }];

        store
            .replace_file_subgraph(&auth, &parsed_for(&auth, "AuthService"), &run.run_id)
            .expect("index auth");
        store
            .replace_file_subgraph(&main, &parsed_main, &run.run_id)
            .expect("index main");
        store
            .replace_file_subgraph(&app, &parsed_app, &run.run_id)
            .expect("index app");
        store
            .rebuild_scope_import_edges(&ctx)
            .expect("rebuild dependencies");

        let impact = store
            .file_impact(&ctx, Path::new("src/auth.rs"))
            .expect("impact")
            .expect("file exists");

        assert!(
            impact
                .impacted_files
                .iter()
                .any(|file| file.relative_path == Path::new("src/main.rs") && file.depth == 1)
        );
        assert!(
            impact
                .impacted_files
                .iter()
                .any(|file| file.relative_path == Path::new("src/app.rs") && file.depth == 2)
        );
    }
}
