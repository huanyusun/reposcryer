# Kuzu Store

Phase 3 uses Kuzu as the single source of truth for persistent graph data, file-level incremental indexing state, and derived file dependency edges.

## Why Kuzu

RepoScryer will rely on graph expansion for impact analysis and later context generation. Introducing Kuzu in Phase 2 stabilized the node and relationship model; Phase 3 now extends that store with derived file dependency edges.

Phase 3 still does not use SQLite or Tantivy. A second metadata database would add consistency work around file fingerprints, index runs, graph replacement, and derived dependency edges. A search index is also premature while symbols, paths, chunks, and graph neighborhoods are still being shaped.

## Schema

Node tables:

- `SchemaMeta`
- `Project`
- `Worktree`
- `IndexScope`
- `File`
- `Symbol`
- `Import`
- `Chunk`
- `Warning`
- `IndexRun`

Relationship tables:

- `Project HAS_WORKTREE Worktree`
- `Worktree HAS_SCOPE IndexScope`
- `IndexScope CONTAINS_FILE File`
- `File DEFINES Symbol`
- `File HAS_IMPORT Import`
- `File HAS_CHUNK Chunk`
- `Symbol HAS_CHUNK Chunk`
- `File HAS_WARNING Warning`
- `IndexRun TOUCHED_FILE File`
- `File IMPORTS_FILE File`

Phase 3 does not create `CALLS` relationships.

`File` nodes store the fields needed for file-level incremental indexing:

- `file_id`
- `repo_id`
- `project_id`
- `worktree_id`
- `scope_id`
- `path`
- `relative_path`
- `absolute_path`
- `language`
- `sha256`
- `size_bytes`
- `mtime_seconds`
- `status`
- `parser_version`
- `graph_schema_version`
- `chunker_version`
- `last_indexed_run_id`
- `created_at`
- `updated_at`
- `deleted_at`

`IndexRun` nodes store run identity, scope identity, parser/schema/chunker versions, status, timestamps, error, and change counters.

`file_id` is generated from `project_id + scope_id + normalized relative path` in Phase 3, so the same path in different scopes does not collide.

`IMPORTS_FILE` relationship properties store the raw import target, confidence score, confidence label, and evidence string. Edges are rebuilt from `Import` nodes and active `File` nodes after an index run.

## Incremental Index Flow

`reposcryer index <path>` scans the repo and compares the scan against `File` records in Kuzu for the current `IndexScope`.

- `Added`: current scan has the file, Kuzu does not.
- `Modified`: current scan and Kuzu both have the file, but `sha256` differs.
- `Unchanged`: `sha256`, parser version, schema version, and chunker version match.
- `Deleted`: Kuzu has the file, but the current scan does not.
- `Skipped`: binary or oversized scanned files. Ignored directories and unsupported language files are omitted from scans to keep `changed` output focused.
- `ReindexNeeded`: `sha256` matches, but parser, schema, or chunker version changed.

`Added`, `Modified`, and `ReindexNeeded` files are read, parsed, and passed to `replace_file_subgraph`. That operation removes stale local children for the file, upserts the `File` node, recreates `Symbol`, `Import`, `Chunk`, and `Warning` nodes, recreates local edges, and records an `IndexRun TOUCHED_FILE` edge.

After changed files are applied, `rebuild_scope_import_edges` deletes existing derived `IMPORTS_FILE` edges for the current scope and recreates them from stored imports. The current resolver is intentionally narrow and only resolves explicit local Rust module paths to indexed files.

`file_neighbors`, `file_impact`, and `scope_graph_summary` query the Kuzu graph directly. `file_impact` intentionally walks reverse file dependencies only; it does not imply symbol-level or call-level impact. `scope_graph_summary` reports current scope counts from Kuzu rather than reading Phase 1 export files.

## Soft Delete

Deleted files are not hard-deleted. `mark_file_deleted` sets `File.status = deleted`, sets `deleted_at`, removes the file's local outgoing subgraph, removes file dependency edges involving that file, and records the touched-file edge. This preserves index run history and allows future garbage collection or branch-aware recovery.

## IndexRun Recovery

The Kuzu Rust API is used with an idempotent two-phase strategy rather than a broad transaction:

1. Create `IndexRun(status = running)`.
2. Apply each changed file with idempotent replacement.
3. Mark the run `completed` with stats.
4. If a step fails, mark the run `failed` and keep the error.

The next index run rescans the filesystem and rebuilds the incremental plan from current Kuzu state, so failed runs do not need a special replay log in Phase 3.

## Current Kuzu Limits

- The implementation uses short-lived connections for simplicity.
- Store connections currently cap Kuzu `max_db_size` at 16 GiB and use one query thread. This avoids excessive virtual memory mapping in local tests and early CLI usage. Large-repo tuning is deferred.
- Schema migration is conservative. If `.reposcryer/kuzu` has an older `SchemaMeta.schema_version`, Phase 3 rebuilds the local Kuzu database from the current filesystem scan.
- Query construction currently uses escaped literals because the store API surface is small.
- Kuzu Rust crate `0.11.3` pins `cxx = 1.0.138` but allows newer `cxx-build`; this workspace pins `cxx-build = 1.0.138` so generated bridge symbols match.
