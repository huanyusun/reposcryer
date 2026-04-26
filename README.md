# RepoScryer

Read [AGENTS.md](AGENTS.md) before making non-trivial changes.

RepoScryer is a local repo intelligence engine for AI coding agents. It provides reliable, low-token, queryable, incrementally updated codebase context through a CLI-first workflow backed by Kuzu.

The project is intentionally staged: graph correctness and agent-friendly query APIs come before RAG, MCP, Web UI, or workspace-scale features.

## Current Capabilities

- Rust workspace with focused crate boundaries
- Repo scan with ignore rules, language detection, and file fingerprints
- Heuristic parsers for Rust, Python, JavaScript, TypeScript, Java, and Go
- Kuzu-backed `GraphStore` for schema initialization, file fingerprints, index runs, and file subgraph replacement
- Incremental `index`, `status`, `changed`, and `graph rebuild` CLI flows
- Kuzu-backed file dependency edges for explicit Rust `mod` and local `use` imports
- `explain` CLI flow for stored file context, symbols, raw imports, resolved file dependencies, and warnings
- Export of `graph.json`, `symbols.json`, `repo-map.md`, and `warnings.json`

## Project Direction

RepoScryer is moving toward a local intelligence layer that agents can query before editing code.

- `v0.1`: Kuzu-backed graph index, incremental indexing, file dependency graph, CI/release.
- `v0.2`: JSON query APIs and graph summary.
- `v0.3`: real `.reposcryer/config.toml` loading.
- `v0.4`: stronger Rust resolver fidelity.
- `v0.5`: deterministic context packs for coding agents.
- `v0.6`: broader language resolvers and test graph.
- `v0.7`: diff-aware impact and test suggestions.
- `v0.8`: Skill integration.

See [docs/roadmap.md](docs/roadmap.md) for the product roadmap and [docs/development-roadmap.md](docs/development-roadmap.md) for the implementation plan.

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
cargo run -p reposcryer-cli -- graph summary examples/sample-rust-project
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
reposcryer explain <path> <file> --json
reposcryer graph neighbors <path> <file>
reposcryer graph neighbors <path> <file> --json
reposcryer graph summary <path>
reposcryer graph summary <path> --json
reposcryer impact <path> <file>
reposcryer impact <path> <file> --json
reposcryer graph rebuild <path>
reposcryer map <path>
reposcryer inspect <path>
```

## Development Workflow

All changes should be made on a feature branch and opened as a pull request into `main`. CI runs Rust formatting, clippy, tests, sample CLI commands, and a guard against fake call graph output. Merges to `main` create an automated GitHub release with Linux, macOS, and Windows CLI artifacts. Linux and macOS artifacts are Unix-like `tar.gz` packages; Windows is published as a `zip`. See [CONTRIBUTING.md](CONTRIBUTING.md).

Release artifact names:

- `reposcryer-linux-x86_64.tar.gz`
- `reposcryer-macos-x86_64.tar.gz`
- `reposcryer-macos-aarch64.tar.gz`
- `reposcryer-windows-x86_64.zip`

## Current Boundaries

Phase 3 does not implement RAG, workspace multi-project management, Web UI, MCP, embeddings, SQLite, or Tantivy. `EdgeKind::Calls` remains a model variant only; RepoScryer does not emit call graph edges without reliable evidence.

See [docs/kuzu-store.md](docs/kuzu-store.md), [docs/architecture.md](docs/architecture.md), [docs/roadmap.md](docs/roadmap.md), [docs/development-roadmap.md](docs/development-roadmap.md), and [docs/limitations.md](docs/limitations.md) for detail.
