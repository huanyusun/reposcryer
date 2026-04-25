# Roadmap

## Phase 0 + Phase 1

- Rust workspace skeleton
- Full indexing loop
- Multi-language heuristic parsing
- Export artifacts for downstream agent consumption

## Phase 2

- Kuzu single source of truth
- `reposcryer-store` crate
- `GraphStore` trait and `KuzuGraphStore`
- Kuzu schema initialization and schema version record
- File fingerprint persistence
- File-level incremental plan generation
- Idempotent file subgraph replacement
- Soft delete for deleted files
- `IndexRun` tracking
- CLI `index`, `status`, `changed`, and `graph rebuild`

## Phase 3

- Kuzu `IMPORTS_FILE` relationship for resolved local file dependencies
- Rust `mod` declaration extraction as raw imports
- Path-based Rust import resolver for explicit local modules
- Idempotent dependency edge rebuild after indexing
- `reposcryer explain <path> <file>` for stored file context
- `reposcryer graph neighbors <path> <file>` for direct incoming/outgoing file dependencies
- `reposcryer impact <path> <file>` for reverse file dependency traversal
- Continued ban on fabricated `CALLS` edges

## Later Phases

- Better parsing fidelity and resolver passes
- Broader file dependency graph and reliable import resolution across more languages
- Impact analysis and graph traversal queries
- Retrieval and context pack generation
- Workspace and multi-project management
- Agent-facing integrations such as Skill packaging
- Optional Web UI, MCP adapter, Tantivy, and embedding provider
