use anyhow::Result;
use reposcryer_core::Language;
use std::path::Path;

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
    pub fn from_path(_path: &Path) -> Result<Self> {
        Ok(Self::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
