use anyhow::{Context, Result, anyhow, bail};
use reposcryer_core::Language;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

pub const CONFIG_DIR: &str = ".reposcryer";
pub const CONFIG_FILE: &str = "config.toml";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoScryerConfig {
    pub output_dir: String,
    pub max_file_size_bytes: u64,
    pub ignored_dirs: Vec<String>,
    pub enabled_languages: Vec<Language>,
}

impl Default for RepoScryerConfig {
    fn default() -> Self {
        Self {
            output_dir: ".reposcryer".to_string(),
            max_file_size_bytes: 1_048_576,
            ignored_dirs: vec![
                ".git".to_string(),
                ".reposcryer".to_string(),
                "node_modules".to_string(),
                "target".to_string(),
                "dist".to_string(),
                "build".to_string(),
                "vendor".to_string(),
            ],
            enabled_languages: vec![
                Language::Rust,
                Language::Python,
                Language::JavaScript,
                Language::TypeScript,
                Language::Java,
                Language::Go,
            ],
        }
    }
}

impl RepoScryerConfig {
    pub fn from_path(path: &Path) -> Result<Self> {
        let config_path = Self::config_path(path);
        if !config_path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&config_path)
            .with_context(|| format!("failed to read {}", config_path.display()))?;
        Self::from_toml_str(&content)
            .map_err(|error| anyhow!("failed to parse {}: {error}", config_path.display()))
    }

    pub fn config_path(root: &Path) -> PathBuf {
        root.join(CONFIG_DIR).join(CONFIG_FILE)
    }

    pub fn write_default_file(root: &Path, overwrite: bool) -> Result<bool> {
        let config_path = Self::config_path(root);
        if config_path.exists() && !overwrite {
            return Ok(false);
        }

        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        fs::write(&config_path, Self::default().to_toml_string())
            .with_context(|| format!("failed to write {}", config_path.display()))?;
        Ok(true)
    }

    fn from_toml_str(content: &str) -> Result<Self> {
        let raw: RawConfig = toml::from_str(content)?;
        let mut config = Self::default();

        if let Some(output_dir) = raw.output_dir {
            if output_dir.trim().is_empty() {
                bail!("output_dir must not be empty");
            }
            config.output_dir = output_dir;
        }

        if let Some(max_file_size_bytes) = raw.max_file_size_bytes {
            if max_file_size_bytes == 0 {
                bail!("max_file_size_bytes must be greater than zero");
            }
            config.max_file_size_bytes = max_file_size_bytes;
        }

        if let Some(ignored_dirs) = raw.ignored_dirs {
            if ignored_dirs.iter().any(|entry| entry.trim().is_empty()) {
                bail!("ignored_dirs must not contain empty entries");
            }
            for ignored_dir in ignored_dirs {
                if !config
                    .ignored_dirs
                    .iter()
                    .any(|existing| existing == &ignored_dir)
                {
                    config.ignored_dirs.push(ignored_dir);
                }
            }
        }

        if let Some(enabled_languages) = raw.enabled_languages {
            if enabled_languages.is_empty() {
                bail!("enabled_languages must not be empty");
            }
            config.enabled_languages = enabled_languages
                .iter()
                .map(|language| parse_language(language))
                .collect::<Result<Vec<_>>>()?;
        }

        Ok(config)
    }

    fn to_toml_string(&self) -> String {
        format!(
            "# RepoScryer configuration\noutput_dir = \"{}\"\nmax_file_size_bytes = {}\nignored_dirs = [{}]\nenabled_languages = [{}]\n",
            escape_toml_string(&self.output_dir),
            self.max_file_size_bytes,
            quote_string_list(&self.ignored_dirs),
            self.enabled_languages
                .iter()
                .map(Language::as_str)
                .map(|language| format!("\"{}\"", language))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

#[derive(Debug, Deserialize)]
struct RawConfig {
    output_dir: Option<String>,
    max_file_size_bytes: Option<u64>,
    ignored_dirs: Option<Vec<String>>,
    enabled_languages: Option<Vec<String>>,
}

fn parse_language(language: &str) -> Result<Language> {
    match language {
        "rust" | "rs" => Ok(Language::Rust),
        "python" | "py" => Ok(Language::Python),
        "javascript" | "js" => Ok(Language::JavaScript),
        "typescript" | "ts" => Ok(Language::TypeScript),
        "java" => Ok(Language::Java),
        "go" => Ok(Language::Go),
        other => bail!("unknown language in enabled_languages: {other}"),
    }
}

fn quote_string_list(values: &[String]) -> String {
    values
        .iter()
        .map(|value| format!("\"{}\"", escape_toml_string(value)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn escape_toml_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn default_config_uses_dot_reposcryer() {
        let config = RepoScryerConfig::default();
        assert_eq!(config.output_dir, ".reposcryer");
    }

    #[test]
    fn default_ignored_dirs_cover_required_defaults() {
        let config = RepoScryerConfig::default();
        for dir in [".git", "node_modules", "target", "dist", "build", "vendor"] {
            assert!(config.ignored_dirs.iter().any(|candidate| candidate == dir));
        }
    }

    #[test]
    fn missing_config_file_returns_defaults() {
        let dir = tempdir().expect("tempdir");
        let config = RepoScryerConfig::from_path(dir.path()).expect("config");

        assert_eq!(config, RepoScryerConfig::default());
    }

    #[test]
    fn parses_dot_reposcryer_config_toml() {
        let dir = tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join(".reposcryer")).expect("config dir");
        fs::write(
            dir.path().join(".reposcryer/config.toml"),
            r#"
output_dir = ".cache/reposcryer"
max_file_size_bytes = 128
ignored_dirs = [".git", ".reposcryer", "generated"]
enabled_languages = ["rust", "python"]
"#,
        )
        .expect("write config");

        let config = RepoScryerConfig::from_path(dir.path()).expect("config");

        assert_eq!(config.output_dir, ".cache/reposcryer");
        assert_eq!(config.max_file_size_bytes, 128);
        assert!(config.ignored_dirs.iter().any(|dir| dir == ".git"));
        assert!(config.ignored_dirs.iter().any(|dir| dir == ".reposcryer"));
        assert!(config.ignored_dirs.iter().any(|dir| dir == "generated"));
        assert_eq!(config.enabled_languages, [Language::Rust, Language::Python]);
    }

    #[test]
    fn invalid_language_is_rejected() {
        let dir = tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join(".reposcryer")).expect("config dir");
        fs::write(
            dir.path().join(".reposcryer/config.toml"),
            r#"enabled_languages = ["rust", "brainfuck"]"#,
        )
        .expect("write config");

        let error = RepoScryerConfig::from_path(dir.path()).expect_err("invalid config");

        assert!(error.to_string().contains("unknown language"));
    }

    #[test]
    fn write_default_config_does_not_overwrite_existing_file() {
        let dir = tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join(".reposcryer")).expect("config dir");
        let config_path = dir.path().join(".reposcryer/config.toml");
        fs::write(&config_path, "max_file_size_bytes = 64\n").expect("write config");

        RepoScryerConfig::write_default_file(dir.path(), false).expect("write default");

        assert_eq!(
            fs::read_to_string(config_path).expect("read config"),
            "max_file_size_bytes = 64\n"
        );
    }
}
