use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPathFilterOptions {
    pub respect_gitignore: bool,
    pub respect_mempalignore: bool,
    pub custom_ignore_files: Vec<PathBuf>,
}

impl Default for ProjectPathFilterOptions {
    fn default() -> Self {
        Self {
            respect_gitignore: true,
            respect_mempalignore: true,
            custom_ignore_files: Vec::new(),
        }
    }
}

#[derive(Debug, Error)]
pub enum ProjectPathFilterError {
    #[error("custom ignore file does not exist: {path}")]
    MissingCustomIgnoreFile { path: PathBuf },
    #[error("project traversal failed for {path}")]
    Walk {
        path: PathBuf,
        #[source]
        source: ignore::Error,
    },
}

pub fn project_walk(
    root: &Path,
    options: &ProjectPathFilterOptions,
) -> Result<ignore::Walk, ProjectPathFilterError> {
    for path in &options.custom_ignore_files {
        if !path.is_file() {
            return Err(ProjectPathFilterError::MissingCustomIgnoreFile { path: path.clone() });
        }
    }

    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(false)
        .git_ignore(options.respect_gitignore)
        .git_exclude(options.respect_gitignore)
        .git_global(options.respect_gitignore)
        .require_git(false)
        .parents(true);

    let mempalignore = root.join(".mempalignore");
    if options.respect_mempalignore && mempalignore.is_file() {
        builder.add_ignore(mempalignore);
    }

    for path in &options.custom_ignore_files {
        builder.add_ignore(path);
    }

    builder.filter_entry(|entry| !is_hard_skipped_dir(entry.path()));
    Ok(builder.build())
}

pub fn is_hard_skipped_dir(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| matches!(name, ".git" | "target" | "node_modules"))
        .unwrap_or(false)
}

pub fn collect_relative_paths(
    root: &Path,
    options: &ProjectPathFilterOptions,
) -> Result<Vec<String>, ProjectPathFilterError> {
    let mut paths = Vec::new();
    for entry in project_walk(root, options)? {
        let entry = entry.map_err(|source| ProjectPathFilterError::Walk {
            path: root.to_path_buf(),
            source,
        })?;
        if entry.path() == root {
            continue;
        }
        if let Ok(relative) = entry.path().strip_prefix(root) {
            paths.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
    paths.sort();
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write(path: &std::path::Path, content: &str) {
        fs::write(path, content).expect("write fixture");
    }

    #[test]
    fn default_walk_respects_gitignore_and_mempalignore() {
        let tmp = TempDir::new().expect("tempdir");
        let root = tmp.path();
        fs::create_dir_all(root.join("src/auth")).expect("src");
        fs::create_dir_all(root.join("dist/build-room")).expect("dist");
        fs::create_dir_all(root.join("generated/gen-room")).expect("generated");
        fs::create_dir_all(root.join("target/release-room")).expect("target");
        write(&root.join(".gitignore"), "dist/\n");
        write(&root.join(".mempalignore"), "generated/\n");

        let paths = collect_relative_paths(root, &ProjectPathFilterOptions::default())
            .expect("walk succeeds");

        assert!(paths.iter().any(|path| path == "src/auth"));
        assert!(!paths.iter().any(|path| path.starts_with("dist/")));
        assert!(!paths.iter().any(|path| path.starts_with("generated/")));
        assert!(!paths.iter().any(|path| path.starts_with("target/")));
    }

    #[test]
    fn no_gitignore_keeps_mempalignore() {
        let tmp = TempDir::new().expect("tempdir");
        let root = tmp.path();
        fs::create_dir_all(root.join("dist/build-room")).expect("dist");
        fs::create_dir_all(root.join("generated/gen-room")).expect("generated");
        write(&root.join(".gitignore"), "dist/\n");
        write(&root.join(".mempalignore"), "generated/\n");

        let paths = collect_relative_paths(
            root,
            &ProjectPathFilterOptions {
                respect_gitignore: false,
                ..ProjectPathFilterOptions::default()
            },
        )
        .expect("walk succeeds");

        assert!(paths.iter().any(|path| path.starts_with("dist/")));
        assert!(!paths.iter().any(|path| path.starts_with("generated/")));
    }

    #[test]
    fn custom_ignore_file_is_validated_and_applied() {
        let tmp = TempDir::new().expect("tempdir");
        let root = tmp.path();
        let custom = root.join("custom.ignore");
        fs::create_dir_all(root.join("notes/api")).expect("notes");
        write(&custom, "notes/\n");

        let paths = collect_relative_paths(
            root,
            &ProjectPathFilterOptions {
                custom_ignore_files: vec![custom],
                ..ProjectPathFilterOptions::default()
            },
        )
        .expect("walk succeeds");

        assert!(!paths.iter().any(|path| path.starts_with("notes/")));
    }

    #[test]
    fn missing_custom_ignore_file_is_an_error() {
        let tmp = TempDir::new().expect("tempdir");
        let missing = tmp.path().join("missing.ignore");
        let err = collect_relative_paths(
            tmp.path(),
            &ProjectPathFilterOptions {
                custom_ignore_files: vec![missing.clone()],
                ..ProjectPathFilterOptions::default()
            },
        )
        .expect_err("missing ignore file should fail");

        assert!(err.to_string().contains(&missing.display().to_string()));
    }
}
