# RepoScryer

Read [AGENTS.md](AGENTS.md) before making non-trivial changes.

RepoScryer is a local repo intelligence engine for AI coding agents. Phase 3 builds on the Kuzu store with reliable file-level dependency edges while preserving the Phase 1 export loop.

## Current Capabilities

- Rust workspace with focused crate boundaries
- Repo scan with ignore rules, language detection, and file fingerprints
- Heuristic parsers for Rust, Python, JavaScript, TypeScript, Java, and Go
- Kuzu-backed `GraphStore` for schema initialization, file fingerprints, index runs, and file subgraph replacement
- Incremental `index`, `status`, `changed`, and `graph rebuild` CLI flows
- Kuzu-backed file dependency edges for explicit Rust `mod` and local `use` imports
- `explain` CLI flow for stored file context, symbols, raw imports, resolved file dependencies, and warnings
- Export of `graph.json`, `symbols.json`, `repo-map.md`, and `warnings.json`

## Output Layout

```plain text
<repo>/.reposcryer/
├── config.toml
├── state.json
├── exports/
│   ├── graph.json
│   ├── symbols.json
│   ├── repo-map.md
│   └── warnings.json
└── kuzu/
```

## Quick Start

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run -p reposcryer-cli -- index examples/sample-rust-project
cargo run -p reposcryer-cli -- status examples/sample-rust-project
cargo run -p reposcryer-cli -- changed examples/sample-rust-project
cargo run -p reposcryer-cli -- explain examples/sample-rust-project src/main.rs
cargo run -p reposcryer-cli -- graph neighbors examples/sample-rust-project src/main.rs
cargo run -p reposcryer-cli -- impact examples/sample-rust-project src/auth.rs
cargo run -p reposcryer-cli -- graph rebuild examples/sample-rust-project
```

## CLI

```bash
reposcryer index <path>
reposcryer index <path> --full
reposcryer index <path> --refresh
reposcryer status <path>
reposcryer changed <path>
reposcryer explain <path> <file>
reposcryer graph neighbors <path> <file>
reposcryer impact <path> <file>
reposcryer graph rebuild <path>
reposcryer map <path>
reposcryer inspect <path>
```

## Development Workflow

All changes should be made on a feature branch and opened as a pull request into `main`. CI runs Rust formatting, clippy, tests, sample CLI commands, and a guard against fake call graph output. Merges to `main` create an automated GitHub release. See [CONTRIBUTING.md](CONTRIBUTING.md).

## Current Boundaries

Phase 3 does not implement RAG, workspace multi-project management, Web UI, MCP, embeddings, SQLite, or Tantivy. `EdgeKind::Calls` remains a model variant only; RepoScryer does not emit call graph edges without reliable evidence.

See [docs/kuzu-store.md](docs/kuzu-store.md), [docs/architecture.md](docs/architecture.md), and [docs/limitations.md](docs/limitations.md) for detail.
