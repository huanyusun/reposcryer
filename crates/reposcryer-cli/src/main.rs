use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use reposcryer_config::RepoScryerConfig;
use reposcryer_core::{
    FileChangeKind, IndexContext, IndexStats, RepoIndex, RepoStatus, project_id_from_repo,
    scope_id_from_path, worktree_id_from_path,
};
use reposcryer_export::{
    export_repo_index, load_graph, load_repo_map, load_symbols, load_warnings,
};
use reposcryer_graph::build_repo_index;
use reposcryer_ingest::{read_source, scan_repo};
use reposcryer_parser::{CHUNKER_VERSION, PARSER_VERSION, ParserRegistry};
use reposcryer_store::{GraphStore, KuzuGraphStore, STORE_SCHEMA_VERSION};
use serde_json::json;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(name = "reposcryer")]
#[command(about = "Local repo intelligence engine for AI coding agents")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    Index(IndexCommand),
    Status {
        path: PathBuf,
    },
    Changed {
        path: PathBuf,
    },
    Explain(FileQueryCommand),
    Impact(FileQueryCommand),
    Graph {
        #[command(subcommand)]
        command: GraphCommand,
    },
    Map {
        path: PathBuf,
    },
    Inspect {
        path: PathBuf,
    },
}

#[derive(Args, Debug)]
struct IndexCommand {
    path: PathBuf,
    #[arg(long)]
    full: bool,
    #[arg(long)]
    refresh: bool,
}

#[derive(Subcommand, Debug)]
enum GraphCommand {
    Rebuild { path: PathBuf },
    Neighbors(FileQueryCommand),
}

#[derive(Args, Debug)]
struct FileQueryCommand {
    path: PathBuf,
    file: PathBuf,
    #[arg(long)]
    json: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Index(command) => run_index(&command.path, command.full, command.refresh),
        Command::Status { path } => run_status(&path),
        Command::Changed { path } => run_changed(&path),
        Command::Explain(command) => run_explain(&command.path, &command.file, command.json),
        Command::Impact(command) => run_impact(&command.path, &command.file, command.json),
        Command::Graph {
            command: GraphCommand::Rebuild { path },
        } => run_index(&path, true, false),
        Command::Graph {
            command: GraphCommand::Neighbors(command),
        } => run_graph_neighbors(&command.path, &command.file, command.json),
        Command::Map { path } => {
            print!("{}", load_repo_map(&path)?);
            Ok(())
        }
        Command::Inspect { path } => {
            let repo_map = load_repo_map(&path)?;
            let graph = load_graph(&path)?;
            let symbols = load_symbols(&path)?;
            let warnings = load_warnings(&path)?;
            println!(
                "repo-map.md\n{}\ngraph.json bytes: {}\nsymbols.json bytes: {}\nwarnings.json bytes: {}",
                first_nonempty_line(&repo_map).unwrap_or("empty"),
                graph.len(),
                symbols.len(),
                warnings.len()
            );
            Ok(())
        }
    }
}

fn run_index(path: &Path, full: bool, refresh: bool) -> Result<()> {
    let config = RepoScryerConfig::from_path(path)?;
    let scan = scan_repo(path, &config)?;
    let ctx = index_context(&scan.repo_root);
    let store = store_for_root(&scan.repo_root, &config);

    if full {
        reset_kuzu_dir(&scan.repo_root, &config)?;
        store.reset_database()?;
    }

    let run = store.begin_index_run(&ctx)?;
    let plan = store.build_incremental_plan(&scan, &ctx)?;
    let mut stats = plan.stats();
    stats.scanned_files = scan.files.len();

    let result = if refresh {
        Ok(0_usize)
    } else {
        apply_incremental_plan(&store, &scan, &plan, &run.run_id)
    };

    match result {
        Ok(warning_count) => {
            stats.warnings = warning_count;
            if !refresh {
                store.rebuild_scope_import_edges(&ctx)?;
            }
            store.complete_index_run(&run.run_id, &stats)?;
            write_runtime_files(path, &config, &run.run_id.0, "completed", &stats)?;
            export_current_snapshot(&scan.repo_root, &config, &scan)?;
            println!(
                "indexed {} ({})",
                scan.repo_root.join(&config.output_dir).display(),
                if full {
                    "full"
                } else if refresh {
                    "refresh"
                } else {
                    "incremental"
                }
            );
            Ok(())
        }
        Err(error) => {
            store.fail_index_run(&run.run_id, &error.to_string())?;
            write_runtime_files(path, &config, &run.run_id.0, "failed", &stats)?;
            Err(error)
        }
    }
}

fn run_status(path: &Path) -> Result<()> {
    let config = RepoScryerConfig::from_path(path)?;
    let scan = scan_repo(path, &config)?;
    let ctx = index_context(&scan.repo_root);
    let store = store_for_root(&scan.repo_root, &config);
    let status = store.repo_status(&scan, &ctx)?;
    print_status(&status);
    Ok(())
}

fn run_changed(path: &Path) -> Result<()> {
    let config = RepoScryerConfig::from_path(path)?;
    let scan = scan_repo(path, &config)?;
    let ctx = index_context(&scan.repo_root);
    let store = store_for_root(&scan.repo_root, &config);
    for change in store.changed_files(&scan, &ctx)? {
        println!(
            "{}\t{}",
            change.kind.as_str(),
            change.relative_path.display()
        );
    }
    Ok(())
}

fn run_explain(path: &Path, file: &Path, json_output: bool) -> Result<()> {
    let config = RepoScryerConfig::from_path(path)?;
    let scan = scan_repo(path, &config)?;
    let ctx = index_context(&scan.repo_root);
    let store = store_for_root(&scan.repo_root, &config);
    let explanation = store
        .explain_file(&ctx, file)?
        .ok_or_else(|| anyhow::anyhow!("file is not indexed: {}", file.display()))?;

    if json_output {
        println!("{}", serde_json::to_string_pretty(&explanation)?);
        return Ok(());
    }

    println!("file: {}", explanation.relative_path.display());
    println!("language: {}", explanation.language.as_str());
    for symbol in explanation.symbols {
        println!("symbol: {:?} {}", symbol.kind, symbol.name);
    }
    for import in explanation.imports {
        println!("import: {} line {}", import.raw_target, import.line);
    }
    for dependency in explanation.dependencies {
        println!(
            "imports_file: {} via {} ({})",
            dependency.to_relative_path.display(),
            dependency.raw_target,
            dependency.evidence.detail
        );
    }
    for warning in explanation.warnings {
        println!("warning: {} {}", warning.stage, warning.message);
    }
    Ok(())
}

fn run_graph_neighbors(path: &Path, file: &Path, json_output: bool) -> Result<()> {
    let config = RepoScryerConfig::from_path(path)?;
    let scan = scan_repo(path, &config)?;
    let ctx = index_context(&scan.repo_root);
    let store = store_for_root(&scan.repo_root, &config);
    let neighbors = store
        .file_neighbors(&ctx, file)?
        .ok_or_else(|| anyhow::anyhow!("file is not indexed: {}", file.display()))?;

    if json_output {
        println!("{}", serde_json::to_string_pretty(&neighbors)?);
        return Ok(());
    }

    println!("file: {}", neighbors.relative_path.display());
    for dependency in neighbors.outgoing {
        println!(
            "outgoing: {} via {}",
            dependency.to_relative_path.display(),
            dependency.raw_target
        );
    }
    for dependency in neighbors.incoming {
        println!(
            "incoming: {} via {}",
            dependency.from_relative_path.display(),
            dependency.raw_target
        );
    }
    Ok(())
}

fn run_impact(path: &Path, file: &Path, json_output: bool) -> Result<()> {
    let config = RepoScryerConfig::from_path(path)?;
    let scan = scan_repo(path, &config)?;
    let ctx = index_context(&scan.repo_root);
    let store = store_for_root(&scan.repo_root, &config);
    let impact = store
        .file_impact(&ctx, file)?
        .ok_or_else(|| anyhow::anyhow!("file is not indexed: {}", file.display()))?;

    if json_output {
        println!("{}", serde_json::to_string_pretty(&impact)?);
        return Ok(());
    }

    println!("file: {}", impact.relative_path.display());
    for impacted in impact.impacted_files {
        println!(
            "impacted: depth={} {} via {}",
            impacted.depth,
            impacted.relative_path.display(),
            impacted.via_relative_path.display()
        );
    }
    Ok(())
}

fn apply_incremental_plan(
    store: &KuzuGraphStore,
    scan: &reposcryer_core::ScanResult,
    plan: &reposcryer_core::IncrementalIndexPlan,
    run_id: &reposcryer_core::RunId,
) -> Result<usize> {
    let registry = ParserRegistry;
    let files_by_id = scan
        .files
        .iter()
        .map(|file| (file.file_id.0.clone(), file))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut warning_count = 0;

    for change in &plan.changes {
        match change.kind {
            FileChangeKind::Added | FileChangeKind::Modified | FileChangeKind::ReindexNeeded => {
                let file = files_by_id
                    .get(&change.file_id.0)
                    .ok_or_else(|| anyhow::anyhow!("missing scanned file {}", change.file_id.0))?;
                let source = read_source(file)
                    .with_context(|| format!("failed to read {}", file.relative_path.display()))?;
                let parsed = registry.parse_or_warn(file, &source);
                warning_count += parsed.warnings.len();
                store.replace_file_subgraph(file, &parsed, run_id)?;
            }
            FileChangeKind::Deleted => {
                store.mark_file_deleted(&change.file_id, run_id)?;
            }
            FileChangeKind::Unchanged | FileChangeKind::Skipped => {}
        }
    }

    Ok(warning_count)
}

fn export_current_snapshot(
    root: &Path,
    config: &RepoScryerConfig,
    scan: &reposcryer_core::ScanResult,
) -> Result<()> {
    let registry = ParserRegistry;
    let mut parsed_files = Vec::new();
    let mut warnings = Vec::new();

    for file in &scan.files {
        let source = read_source(file)
            .with_context(|| format!("failed to read {}", file.relative_path.display()))?;
        let parsed = registry.parse_or_warn(file, &source);
        warnings.extend(parsed.warnings.clone());
        parsed_files.push(parsed);
    }

    let index: RepoIndex = build_repo_index(root, scan.files.clone(), parsed_files, warnings);
    export_repo_index(root, config, &index)?;
    Ok(())
}

fn index_context(root: &Path) -> IndexContext {
    let repo_id = reposcryer_core::repo_id_from_path(root);
    IndexContext {
        repo_id: repo_id.clone(),
        project_id: project_id_from_repo(&repo_id),
        worktree_id: worktree_id_from_path(root),
        scope_id: scope_id_from_path(root),
        repo_root: root.to_path_buf(),
        parser_version: PARSER_VERSION.to_string(),
        schema_version: STORE_SCHEMA_VERSION.to_string(),
        chunker_version: CHUNKER_VERSION.to_string(),
    }
}

fn store_for_root(root: &Path, config: &RepoScryerConfig) -> KuzuGraphStore {
    KuzuGraphStore::new(root.join(&config.output_dir).join("kuzu").join("db"))
}

fn reset_kuzu_dir(root: &Path, config: &RepoScryerConfig) -> Result<()> {
    let kuzu_dir = root.join(&config.output_dir).join("kuzu");
    if kuzu_dir.exists() {
        std::fs::remove_dir_all(&kuzu_dir)
            .with_context(|| format!("failed to remove {}", kuzu_dir.display()))?;
    }
    Ok(())
}

fn write_runtime_files(
    path: &Path,
    config: &RepoScryerConfig,
    run_id: &str,
    status: &str,
    stats: &IndexStats,
) -> Result<()> {
    let output_dir = path.join(&config.output_dir);
    std::fs::create_dir_all(&output_dir)?;
    std::fs::write(
        output_dir.join("config.toml"),
        format!(
            "output_dir = \"{}\"\nmax_file_size_bytes = {}\n",
            config.output_dir, config.max_file_size_bytes
        ),
    )?;
    let state_path = output_dir.join("state.json");
    std::fs::write(
        state_path,
        serde_json::to_string_pretty(&json!({
            "run_id": run_id,
            "status": status,
            "stats": stats,
        }))?,
    )?;
    Ok(())
}

fn print_status(status: &RepoStatus) {
    println!("tracked_files: {}", status.tracked_files);
    println!("scan_files: {}", status.scan_files);
    println!("added: {}", status.added);
    println!("modified: {}", status.modified);
    println!("deleted: {}", status.deleted);
    println!("unchanged: {}", status.unchanged);
    println!("skipped: {}", status.skipped);
    println!("reindex_needed: {}", status.reindex_needed);
}

fn first_nonempty_line(content: &str) -> Option<&str> {
    content.lines().find(|line| !line.trim().is_empty())
}
