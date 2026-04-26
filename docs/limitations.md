# Limitations

Phase 3 trades feature breadth for a durable Kuzu-backed indexing core plus reliable file-level graph enhancement.

- Parsers are heuristic and may miss complex constructs, macro-expanded items, nested scopes, or uncommon formatting.
- JavaScript and TypeScript support focus on common `import`, `require`, `function`, `class`, and arrow-function forms.
- Java and Go parsing is intentionally shallow and does not attempt full signature or package resolution.
- Import nodes keep raw targets. File-level import resolution currently supports explicit local Rust module paths only.
- Rust dependency resolution is heuristic and path-based. It does not run `rustc`, expand macros, evaluate `cfg`, resolve Cargo workspace packages, or parse complex grouped imports such as `use crate::{a, b};`.
- Rust resolver support is intentionally file-level. It resolves indexed module file paths, not item-level definitions inside those files.
- `reposcryer impact` is file-level reverse dependency analysis. It reports files that may be affected through resolved imports, not guaranteed runtime behavior.
- `reposcryer context --budget` is an approximate character budget, not a tokenizer-backed budget.
- Context packs include deterministic excerpts and graph data only. They do not summarize semantically, call an LLM, use embeddings, or perform RAG.
- `EdgeKind::Calls` exists in the model but is intentionally not emitted in Phase 3.
- File-level incremental indexing does not yet rebuild inbound weak references from other files when one file changes.
- Project, worktree, and scope IDs are persisted, and file IDs are scope-bound. Branch-aware and workspace-aware scope selection is not yet implemented.
- `reposcryer index --refresh` currently validates and records a run without reparsing changed files.
- Configured ignored directories and unsupported languages are silently omitted from scans; `Skipped` currently reports binary and oversized files to avoid noisy `.reposcryer` self-reporting.
- Config discovery is fixed to `<repo>/.reposcryer/config.toml`. A custom `output_dir` changes runtime output locations but does not move the config file path.
- Configured `ignored_dirs` are merged with default safety ignores. RepoScryer does not currently support removing `.git` or `.reposcryer` from the ignored directory set.
- Kuzu transaction support is not used yet; Phase 3 uses idempotent per-file replacement plus `IndexRun` status for recovery.
- Kuzu database access should be treated as single-process in the current CLI. Running multiple RepoScryer commands against the same `.reposcryer/kuzu/db` concurrently can hit Kuzu file locks.
- Kuzu store connections currently cap `max_db_size` at 16 GiB and use one query thread to keep local and CI runs stable. Large-repo tuning is deferred.
- Kuzu schema migration is currently rebuild-based. If the stored schema version differs from the current Phase 3 schema, the local Kuzu database is rebuilt from the current filesystem scan.
- Kuzu Rust crate `0.11.3` requires `cxx-build` to match its pinned `cxx` bridge version. The workspace pins `cxx-build = 1.0.138` to avoid bridge symbol mismatches.

These constraints are deliberate. The goal is to document uncertainty rather than overstate accuracy.
