use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use reposcryer_config::RepoScryerConfig;
use reposcryer_context::{ContextInput, ContextMode, build_context_pack, render_markdown};
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
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    Context(ContextCommand),
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
    Summary(SummaryCommand),
}

#[derive(Subcommand, Debug)]
enum ConfigCommand {
    Init(ConfigInitCommand),
}

#[derive(Args, Debug)]
struct FileQueryCommand {
    path: PathBuf,
    file: PathBuf,
    #[arg(long)]
    json: bool,
}

#[derive(Args, Debug)]
struct SummaryCommand {
    path: PathBuf,
    #[arg(long)]
    json: bool,
}

#[derive(Args, Debug)]
struct ConfigInitCommand {
    path: PathBuf,
    #[arg(long)]
    force: bool,
}

#[derive(Args, Debug)]
struct ContextCommand {
    path: PathBuf,
    #[arg(long)]
    file: PathBuf,
    #[arg(long, default_value = "explain")]
    mode: String,
    #[arg(long, default_value_t = 4000)]
    budget: usize,
    #[arg(long)]
    json: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Index(command) => run_index(&command.path, command.full, command.refresh),
        Command::Status { path } => run_status(&path),
        Command::Changed { path } => run_changed(&path),
        Command::Config {
            command: ConfigCommand::Init(command),
        } => run_config_init(&command.path, command.force),
        Command::Context(command) => run_context(
            &command.path,
            &command.file,
            &command.mode,
            command.budget,
            command.json,
        ),
        Command::Explain(command) => run_explain(&command.path, &command.file, command.json),
        Command::Impact(command) => run_impact(&command.path, &command.file, command.json),
        Command::Graph {
            command: GraphCommand::Rebuild { path },
        } => run_index(&path, true, false),
        Command::Graph {
            command: GraphCommand::Neighbors(command),
        } => run_graph_neighbors(&command.path, &command.file, command.json),
        Command::Graph {
            command: GraphCommand::Summary(command),
        } => run_graph_summary(&command.path, command.json),
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

fn run_config_init(path: &Path, force: bool) -> Result<()> {
    let written = RepoScryerConfig::write_default_file(path, force)?;
    let config_path = RepoScryerConfig::config_path(path);
    if written {
        println!("created {}", config_path.display());
    } else {
        println!("exists {}", config_path.display());
    }
    Ok(())
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

fn run_context(
    path: &Path,
    file: &Path,
    mode: &str,
    budget: usize,
    json_output: bool,
) -> Result<()> {
    let config = RepoScryerConfig::from_path(path)?;
    let scan = scan_repo(path, &config)?;
    let ctx = index_context(&scan.repo_root);
    let store = store_for_root(&scan.repo_root, &config);
    let explanation = store
        .explain_file(&ctx, file)?
        .ok_or_else(|| anyhow::anyhow!("file is not indexed: {}", file.display()))?;
    let neighbors = store
        .file_neighbors(&ctx, file)?
        .ok_or_else(|| anyhow::anyhow!("file is not indexed: {}", file.display()))?;
    let impact = store
        .file_impact(&ctx, file)?
        .ok_or_else(|| anyhow::anyhow!("file is not indexed: {}", file.display()))?;
    let source = read_context_source(&scan, file)?;
    let repo_map = read_repo_map_for_config(&scan.repo_root, &config).unwrap_or_default();
    let pack = build_context_pack(ContextInput {
        mode: parse_context_mode(mode)?,
        budget,
        explanation,
        neighbors,
        impact,
        source,
        repo_map,
    })?;

    if json_output {
        println!("{}", serde_json::to_string_pretty(&pack)?);
    } else {
        print!("{}", render_markdown(&pack));
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

fn run_graph_summary(path: &Path, json_output: bool) -> Result<()> {
    let config = RepoScryerConfig::from_path(path)?;
    let scan = scan_repo(path, &config)?;
    let ctx = index_context(&scan.repo_root);
    let store = store_for_root(&scan.repo_root, &config);
    let summary = store.scope_graph_summary(&ctx)?;

    if json_output {
        println!("{}", serde_json::to_string_pretty(&summary)?);
        return Ok(());
    }

    println!("scope_id: {}", summary.scope_id.0);
    println!("active_files: {}", summary.active_files);
    println!("deleted_files: {}", summary.deleted_files);
    println!("symbols: {}", summary.symbols);
    println!("imports: {}", summary.imports);
    println!("dependency_edges: {}", summary.dependency_edges);
    println!("warnings: {}", summary.warnings);
    println!("index_runs: {}", summary.index_runs);
    if let Some(run_id) = summary.latest_run_id {
        println!("latest_run_id: {}", run_id.0);
    }
    if let Some(status) = summary.latest_run_status {
        println!("latest_run_status: {status}");
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

fn parse_context_mode(mode: &str) -> Result<ContextMode> {
    match mode {
        "explain" => Ok(ContextMode::Explain),
        "change-plan" => Ok(ContextMode::ChangePlan),
        "review" => Ok(ContextMode::Review),
        other => Err(anyhow::anyhow!(
            "unsupported context mode {other}; expected explain, change-plan, or review"
        )),
    }
}

fn read_context_source(scan: &reposcryer_core::ScanResult, file: &Path) -> Result<String> {
    let requested = normalize_relative_path(file);
    let scanned_file = scan
        .files
        .iter()
        .find(|candidate| normalize_relative_path(&candidate.relative_path) == requested)
        .ok_or_else(|| anyhow::anyhow!("file is not in current scan: {}", file.display()))?;
    read_source(scanned_file)
}

fn read_repo_map_for_config(root: &Path, config: &RepoScryerConfig) -> Result<String> {
    Ok(std::fs::read_to_string(
        root.join(&config.output_dir)
            .join("exports")
            .join("repo-map.md"),
    )?)
}

fn normalize_relative_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
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
    RepoScryerConfig::write_default_file(path, false)?;
    let output_dir = path.join(&config.output_dir);
    std::fs::create_dir_all(&output_dir)?;
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
