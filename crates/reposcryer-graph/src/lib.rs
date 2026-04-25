use reposcryer_core::{
    CodeFile, Confidence, EdgeKind, Evidence, GraphEdge, ImportTargetId, IndexWarning, ParsedFile,
    RepoIndex, edge_id_from_parts, import_target_id,
};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn build_repo_index(
    repo_root: &Path,
    files: Vec<CodeFile>,
    parsed_files: Vec<ParsedFile>,
    warnings: Vec<IndexWarning>,
) -> RepoIndex {
    let repo_id = files
        .first()
        .map(|file| file.repo_id.clone())
        .unwrap_or_else(|| reposcryer_core::repo_id_from_path(repo_root));
    let mut symbols = Vec::new();
    let mut edges = Vec::new();

    for parsed in parsed_files {
        for symbol in &parsed.symbols {
            edges.push(defines_edge(
                &symbol.file_id.0,
                &symbol.symbol_id.0,
                &symbol.name,
            ));
            edges.push(belongs_to_edge(
                &symbol.symbol_id.0,
                &symbol.file_id.0,
                &symbol.name,
            ));
        }
        for import in &parsed.imports {
            let target_id: ImportTargetId = import_target_id(&import.raw_target);
            edges.push(imports_edge(
                &import.from_file_id.0,
                &target_id.0,
                &import.raw_target,
            ));
        }
        symbols.extend(parsed.symbols);
    }

    RepoIndex {
        repo_id,
        repo_root: repo_root.to_path_buf(),
        indexed_at: indexed_at_string(),
        files,
        symbols,
        edges,
        warnings,
    }
}

fn defines_edge(from_id: &str, to_id: &str, symbol_name: &str) -> GraphEdge {
    build_edge(
        from_id,
        to_id,
        EdgeKind::Defines,
        format!("defines:{symbol_name}"),
    )
}

fn belongs_to_edge(from_id: &str, to_id: &str, symbol_name: &str) -> GraphEdge {
    build_edge(
        from_id,
        to_id,
        EdgeKind::BelongsTo,
        format!("belongs_to:{symbol_name}"),
    )
}

fn imports_edge(from_id: &str, to_id: &str, raw_target: &str) -> GraphEdge {
    build_edge(
        from_id,
        to_id,
        EdgeKind::Imports,
        format!("imports:{raw_target}"),
    )
}

fn build_edge(from_id: &str, to_id: &str, kind: EdgeKind, detail: String) -> GraphEdge {
    GraphEdge {
        edge_id: edge_id_from_parts(from_id, to_id, &kind, &detail),
        from_id: from_id.to_string(),
        to_id: to_id.to_string(),
        kind,
        confidence: Confidence {
            score: 1.0,
            label: "high".to_string(),
        },
        evidence: Evidence {
            source: "heuristic-parser".to_string(),
            detail,
        },
    }
}

fn indexed_at_string() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    seconds.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use reposcryer_core::{
        CodeChunk, CodeFile, FileFingerprint, FileId, Import, Language, ParsedFile, Symbol,
        SymbolId, SymbolKind, file_id_from_relative_path, repo_id_from_path,
    };
    use std::path::{Path, PathBuf};

    fn sample_file() -> CodeFile {
        let relative_path = PathBuf::from("src/main.rs");
        CodeFile {
            repo_id: repo_id_from_path(Path::new("/tmp/repo")),
            file_id: file_id_from_relative_path(&relative_path),
            path: PathBuf::from("/tmp/repo/src/main.rs"),
            relative_path,
            language: Language::Rust,
            fingerprint: FileFingerprint {
                sha256: "abc".to_string(),
                size_bytes: 3,
                mtime_seconds: 1,
            },
        }
    }

    #[test]
    fn file_defines_symbol_edge_exists() {
        let file = sample_file();
        let parsed = ParsedFile {
            file_id: file.file_id.clone(),
            symbols: vec![Symbol {
                symbol_id: SymbolId("sym".to_string()),
                file_id: file.file_id.clone(),
                file_path: file.relative_path.clone(),
                name: "run".to_string(),
                kind: SymbolKind::Function,
                range: CodeChunk {
                    start_line: 1,
                    end_line: 1,
                },
                language: Language::Rust,
            }],
            imports: vec![Import {
                import_id: reposcryer_core::ImportId("imp".to_string()),
                from_file_id: FileId(file.file_id.0.clone()),
                raw_target: "crate::auth".to_string(),
                line: 1,
            }],
            chunks: Vec::new(),
            warnings: Vec::new(),
        };

        let index = build_repo_index(Path::new("/tmp/repo"), vec![file], vec![parsed], vec![]);
        assert!(
            index
                .edges
                .iter()
                .any(|edge| edge.kind == EdgeKind::Defines)
        );
        assert!(index.edges.iter().all(|edge| edge.kind != EdgeKind::Calls));
        assert!(index.edges.iter().all(|edge| edge.confidence.score > 0.0));
    }
}
