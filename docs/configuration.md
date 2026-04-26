# Configuration

RepoScryer reads repository configuration from:

```plain text
<repo>/.reposcryer/config.toml
```

Create the file with:

```bash
reposcryer config init <path>
```

By default, `config init` does not overwrite an existing file. Use `--force` only when you intentionally want to replace the current config with defaults.

## Schema

```toml
output_dir = ".reposcryer"
max_file_size_bytes = 1048576
ignored_dirs = [".git", ".reposcryer", "node_modules", "target", "dist", "build", "vendor"]
enabled_languages = ["rust", "python", "javascript", "typescript", "java", "go"]
```

Fields:

- `output_dir`: directory used for RepoScryer runtime state, exports, and Kuzu data.
- `max_file_size_bytes`: files larger than this limit are skipped as `oversized`.
- `ignored_dirs`: directory names omitted from scans. Values are merged with the default safety ignores so `.git` and `.reposcryer` are not scanned accidentally.
- `enabled_languages`: language filters. Supported values are `rust`, `python`, `javascript`, `typescript`, `java`, and `go`.

Missing fields use defaults. Invalid TOML, unknown languages, empty `output_dir`, zero `max_file_size_bytes`, and empty `enabled_languages` are rejected.

## Discovery Limitation

Config discovery is fixed to `<repo>/.reposcryer/config.toml`. A custom `output_dir` changes where runtime state, exports, and Kuzu data are written, but it does not move the config discovery path.
