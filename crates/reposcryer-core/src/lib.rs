use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct RepoId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ProjectId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct WorktreeId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ScopeId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct RunId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct FileId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SymbolId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct EdgeId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ImportId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ImportTargetId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ChunkId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct WarningId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Java,
    Go,
    Unknown,
}

impl Language {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Python => "python",
            Self::JavaScript => "javascript",
            Self::TypeScript => "typescript",
            Self::Java => "java",
            Self::Go => "go",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileFingerprint {
    pub sha256: String,
    pub size_bytes: u64,
    pub mtime_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodeFile {
    pub repo_id: RepoId,
    pub file_id: FileId,
    pub path: PathBuf,
    pub relative_path: PathBuf,
    pub language: Language,
    pub fingerprint: FileFingerprint,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkippedFile {
    pub relative_path: PathBuf,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScanResult {
    pub repo_id: RepoId,
    pub repo_root: PathBuf,
    pub files: Vec<CodeFile>,
    pub skipped_files: Vec<SkippedFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Struct,
    Enum,
    Trait,
    Function,
    Method,
    Class,
    Interface,
    Module,
    Type,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodeChunk {
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Symbol {
    pub symbol_id: SymbolId,
    pub file_id: FileId,
    pub file_path: PathBuf,
    pub name: String,
    pub kind: SymbolKind,
    pub range: CodeChunk,
    pub language: Language,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Import {
    pub import_id: ImportId,
    pub from_file_id: FileId,
    pub raw_target: String,
    pub line: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Chunk {
    pub chunk_id: ChunkId,
    pub file_id: FileId,
    pub symbol_id: Option<SymbolId>,
    pub label: String,
    pub range: CodeChunk,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FileStatus {
    Active,
    Deleted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexWarning {
    pub warning_id: WarningId,
    pub file_id: Option<FileId>,
    pub file_path: PathBuf,
    pub stage: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EdgeKind {
    Defines,
    BelongsTo,
    Imports,
    Calls,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Confidence {
    pub score: f32,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Evidence {
    pub source: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GraphEdge {
    pub edge_id: EdgeId,
    pub from_id: String,
    pub to_id: String,
    pub kind: EdgeKind,
    pub confidence: Confidence,
    pub evidence: Evidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileDependency {
    pub from_file_id: FileId,
    pub to_file_id: FileId,
    pub from_relative_path: PathBuf,
    pub to_relative_path: PathBuf,
    pub raw_target: String,
    pub confidence: Confidence,
    pub evidence: Evidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ParsedFile {
    pub file_id: FileId,
    pub symbols: Vec<Symbol>,
    pub imports: Vec<Import>,
    pub chunks: Vec<Chunk>,
    pub warnings: Vec<IndexWarning>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileExplanation {
    pub file_id: FileId,
    pub relative_path: PathBuf,
    pub language: Language,
    pub symbols: Vec<Symbol>,
    pub imports: Vec<Import>,
    pub dependencies: Vec<FileDependency>,
    pub warnings: Vec<IndexWarning>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileNeighborhood {
    pub file_id: FileId,
    pub relative_path: PathBuf,
    pub outgoing: Vec<FileDependency>,
    pub incoming: Vec<FileDependency>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImpactedFile {
    pub file_id: FileId,
    pub relative_path: PathBuf,
    pub depth: usize,
    pub via_relative_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileImpact {
    pub file_id: FileId,
    pub relative_path: PathBuf,
    pub impacted_files: Vec<ImpactedFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraphSummary {
    pub scope_id: ScopeId,
    pub active_files: usize,
    pub deleted_files: usize,
    pub symbols: usize,
    pub imports: usize,
    pub dependency_edges: usize,
    pub warnings: usize,
    pub index_runs: usize,
    pub latest_run_id: Option<RunId>,
    pub latest_run_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexContext {
    pub repo_id: RepoId,
    pub project_id: ProjectId,
    pub worktree_id: WorktreeId,
    pub scope_id: ScopeId,
    pub repo_root: PathBuf,
    pub parser_version: String,
    pub schema_version: String,
    pub chunker_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum IndexRunStatus {
    Running,
    Completed,
    Failed,
    Interrupted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct IndexStats {
    pub scanned_files: usize,
    pub added: usize,
    pub modified: usize,
    pub unchanged: usize,
    pub deleted: usize,
    pub skipped: usize,
    pub reindex_needed: usize,
    pub warnings: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexRun {
    pub run_id: RunId,
    pub project_id: ProjectId,
    pub worktree_id: WorktreeId,
    pub scope_id: ScopeId,
    pub status: IndexRunStatus,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FileChangeKind {
    Added,
    Modified,
    Unchanged,
    Deleted,
    Skipped,
    ReindexNeeded,
}

impl FileChangeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Modified => "modified",
            Self::Unchanged => "unchanged",
            Self::Deleted => "deleted",
            Self::Skipped => "skipped",
            Self::ReindexNeeded => "reindex_needed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileChange {
    pub file_id: FileId,
    pub relative_path: PathBuf,
    pub kind: FileChangeKind,
    pub fingerprint: Option<FileFingerprint>,
    pub previous_sha256: Option<String>,
    pub current_sha256: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct IncrementalIndexPlan {
    pub project_id: Option<ProjectId>,
    pub worktree_id: Option<WorktreeId>,
    pub scope_id: Option<ScopeId>,
    pub changes: Vec<FileChange>,
}

impl IncrementalIndexPlan {
    pub fn changes_of_kind(&self, kind: FileChangeKind) -> Vec<&FileChange> {
        self.changes
            .iter()
            .filter(|change| change.kind == kind)
            .collect()
    }

    pub fn stats(&self) -> IndexStats {
        let mut stats = IndexStats::default();
        for change in &self.changes {
            match change.kind {
                FileChangeKind::Added => stats.added += 1,
                FileChangeKind::Modified => stats.modified += 1,
                FileChangeKind::Unchanged => stats.unchanged += 1,
                FileChangeKind::Deleted => stats.deleted += 1,
                FileChangeKind::Skipped => stats.skipped += 1,
                FileChangeKind::ReindexNeeded => stats.reindex_needed += 1,
            }
        }
        stats
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RepoStatus {
    pub tracked_files: usize,
    pub scan_files: usize,
    pub added: usize,
    pub modified: usize,
    pub unchanged: usize,
    pub deleted: usize,
    pub skipped: usize,
    pub reindex_needed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileFingerprintRecord {
    pub project_id: ProjectId,
    pub worktree_id: WorktreeId,
    pub scope_id: ScopeId,
    pub file_id: FileId,
    pub relative_path: PathBuf,
    pub fingerprint: FileFingerprint,
    pub parser_version: String,
    pub schema_version: String,
    pub chunker_version: String,
    pub last_indexed_run_id: Option<RunId>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub status: FileStatus,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RepoIndex {
    pub repo_id: RepoId,
    pub repo_root: PathBuf,
    pub indexed_at: String,
    pub files: Vec<CodeFile>,
    pub symbols: Vec<Symbol>,
    pub edges: Vec<GraphEdge>,
    pub warnings: Vec<IndexWarning>,
}

pub fn stable_hash(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0_u8]);
    }
    format!("{:x}", hasher.finalize())
}

pub fn repo_id_from_path(path: &Path) -> RepoId {
    RepoId(stable_hash(&[&path.to_string_lossy()]))
}

pub fn project_id_from_repo(repo_id: &RepoId) -> ProjectId {
    ProjectId(stable_hash(&[&repo_id.0, "project"]))
}

pub fn worktree_id_from_path(path: &Path) -> WorktreeId {
    WorktreeId(stable_hash(&[&path.to_string_lossy(), "worktree"]))
}

pub fn scope_id_from_path(path: &Path) -> ScopeId {
    ScopeId(stable_hash(&[&path.to_string_lossy(), "scope"]))
}

pub fn run_id_for_context(ctx: &IndexContext, seed: &str) -> RunId {
    RunId(stable_hash(&[&ctx.scope_id.0, seed]))
}

pub fn file_id_from_relative_path(path: &Path) -> FileId {
    FileId(stable_hash(&[&path.to_string_lossy()]))
}

pub fn file_id_from_project_scope_path(
    project_id: &ProjectId,
    scope_id: &ScopeId,
    path: &Path,
) -> FileId {
    FileId(stable_hash(&[
        &project_id.0,
        &scope_id.0,
        &path.to_string_lossy(),
    ]))
}

pub fn symbol_id_from_parts(
    file_path: &Path,
    name: &str,
    start_line: usize,
    end_line: usize,
) -> SymbolId {
    SymbolId(stable_hash(&[
        &file_path.to_string_lossy(),
        name,
        &start_line.to_string(),
        &end_line.to_string(),
    ]))
}

pub fn import_id_from_parts(file_id: &FileId, raw_target: &str, line: usize) -> ImportId {
    ImportId(stable_hash(&[&file_id.0, raw_target, &line.to_string()]))
}

pub fn chunk_id_from_parts(
    file_id: &FileId,
    label: &str,
    start_line: usize,
    end_line: usize,
) -> ChunkId {
    ChunkId(stable_hash(&[
        &file_id.0,
        label,
        &start_line.to_string(),
        &end_line.to_string(),
    ]))
}

pub fn warning_id_from_parts(file_path: &Path, stage: &str, message: &str) -> WarningId {
    WarningId(stable_hash(&[&file_path.to_string_lossy(), stage, message]))
}

pub fn import_target_id(raw_target: &str) -> ImportTargetId {
    ImportTargetId(stable_hash(&[raw_target]))
}

pub fn edge_id_from_parts(from_id: &str, to_id: &str, kind: &EdgeKind, detail: &str) -> EdgeId {
    let kind_str = match kind {
        EdgeKind::Defines => "defines",
        EdgeKind::BelongsTo => "belongs_to",
        EdgeKind::Imports => "imports",
        EdgeKind::Calls => "calls",
    };
    EdgeId(stable_hash(&[from_id, to_id, kind_str, detail]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_file_id_from_path() {
        let left = file_id_from_relative_path(Path::new("src/main.rs"));
        let right = file_id_from_relative_path(Path::new("src/main.rs"));
        assert_eq!(left, right);
    }

    #[test]
    fn stable_symbol_id_from_file_name_and_range() {
        let left = symbol_id_from_parts(Path::new("src/lib.rs"), "run", 10, 12);
        let right = symbol_id_from_parts(Path::new("src/lib.rs"), "run", 10, 12);
        assert_eq!(left, right);
    }

    #[test]
    fn graph_edge_contains_confidence_and_evidence() {
        let edge = GraphEdge {
            edge_id: edge_id_from_parts("file", "symbol", &EdgeKind::Defines, "unit-test"),
            from_id: "file".to_string(),
            to_id: "symbol".to_string(),
            kind: EdgeKind::Defines,
            confidence: Confidence {
                score: 1.0,
                label: "high".to_string(),
            },
            evidence: Evidence {
                source: "test".to_string(),
                detail: "unit-test".to_string(),
            },
        };

        assert_eq!(edge.confidence.label, "high");
        assert_eq!(edge.evidence.source, "test");
    }

    #[test]
    fn incremental_plan_stats_count_each_change_kind() {
        let plan = IncrementalIndexPlan {
            project_id: None,
            worktree_id: None,
            scope_id: None,
            changes: vec![
                FileChange {
                    file_id: FileId("a".to_string()),
                    relative_path: PathBuf::from("src/a.rs"),
                    kind: FileChangeKind::Added,
                    fingerprint: None,
                    previous_sha256: None,
                    current_sha256: Some("a".to_string()),
                    reason: None,
                },
                FileChange {
                    file_id: FileId("b".to_string()),
                    relative_path: PathBuf::from("src/b.rs"),
                    kind: FileChangeKind::Modified,
                    fingerprint: None,
                    previous_sha256: Some("old".to_string()),
                    current_sha256: Some("new".to_string()),
                    reason: None,
                },
                FileChange {
                    file_id: FileId("c".to_string()),
                    relative_path: PathBuf::from("src/c.rs"),
                    kind: FileChangeKind::Deleted,
                    fingerprint: None,
                    previous_sha256: Some("c".to_string()),
                    current_sha256: None,
                    reason: None,
                },
                FileChange {
                    file_id: FileId("d".to_string()),
                    relative_path: PathBuf::from("src/d.rs"),
                    kind: FileChangeKind::Unchanged,
                    fingerprint: None,
                    previous_sha256: Some("d".to_string()),
                    current_sha256: Some("d".to_string()),
                    reason: None,
                },
            ],
        };

        let stats = plan.stats();
        assert_eq!(stats.added, 1);
        assert_eq!(stats.modified, 1);
        assert_eq!(stats.deleted, 1);
        assert_eq!(stats.unchanged, 1);
    }

    #[test]
    fn scope_and_worktree_ids_are_stable() {
        let scope_left = scope_id_from_path(Path::new("/tmp/repo"));
        let scope_right = scope_id_from_path(Path::new("/tmp/repo"));
        let worktree_left = worktree_id_from_path(Path::new("/tmp/repo"));
        let worktree_right = worktree_id_from_path(Path::new("/tmp/repo"));

        assert_eq!(scope_left, scope_right);
        assert_eq!(worktree_left, worktree_right);
    }
}
