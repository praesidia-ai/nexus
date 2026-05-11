//! Shared file-system utilities — single source of truth for directory walking.
//!
//! Replaces 12+ duplicate `walk()` implementations scattered across:
//! invariants, invariant_enforcer, taste_engine, code_graph, post_build_intel,
//! mutation_engine, quality_gate, etc.

use std::path::{Path, PathBuf};

/// Directories always skipped during recursive walks.
const SKIP_DIRS: &[&str] = &["node_modules", ".next", "target", ".git", ".nexus", "__pycache__", "dist", ".turbo"];

/// Recursively collect files from a directory, filtering by extension.
///
/// - Skips hidden directories (starting with `.`) and `SKIP_DIRS`
/// - Filters by the provided file extensions (e.g., `&["ts", "tsx", "js"]`)
/// - Returns absolute paths
///
/// # Example
/// ```ignore
/// let files = collect_files_by_ext(project_dir, &["ts", "tsx", "jsx", "css"]);
/// ```
pub fn collect_files_by_ext(dir: &Path, extensions: &[&str]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    walk_recursive(dir, &mut |path| {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if extensions.contains(&ext) {
                files.push(path.to_path_buf());
            }
        }
    });
    files
}

/// Recursively collect ALL non-hidden files from a directory.
pub fn collect_all_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    walk_recursive(dir, &mut |path| {
        if path.is_file() {
            files.push(path.to_path_buf());
        }
    });
    files
}

/// Recursively collect files with their content, filtered by extension.
/// Returns (relative_path, content) pairs.
pub fn collect_files_with_content(dir: &Path, extensions: &[&str]) -> Vec<(String, String)> {
    let mut result = Vec::new();
    let files = collect_files_by_ext(dir, extensions);
    for path in files {
        if let Ok(content) = std::fs::read_to_string(&path) {
            let rel = path.strip_prefix(dir).unwrap_or(&path).display().to_string();
            result.push((rel, content));
        }
    }
    result
}

/// Collect just file names (not paths) from a directory.
pub fn collect_file_names(dir: &Path) -> Vec<String> {
    let mut names = Vec::new();
    walk_recursive(dir, &mut |path| {
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            names.push(name.to_string());
        }
    });
    names
}

/// Core recursive walker — calls `visitor` for every non-directory entry.
fn walk_recursive(dir: &Path, visitor: &mut dyn FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip hidden and known non-source directories
        if name_str.starts_with('.') || SKIP_DIRS.contains(&name_str.as_ref()) {
            continue;
        }

        let path = entry.path();
        if path.is_dir() {
            walk_recursive(&path, visitor);
        } else {
            visitor(&path);
        }
    }
}

/// Check if any source file in a directory contains a given pattern.
///
/// Searches .tsx, .ts, .jsx, .js, .css files recursively.
/// Returns true on first match (short-circuits).
pub fn contains_pattern(dir: &Path, pattern: &str) -> bool {
    let files = collect_files_by_ext(dir, &["tsx", "ts", "jsx", "js", "css"]);
    for path in files {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if content.contains(pattern) {
                return true;
            }
        }
    }
    false
}

/// Build a tree-formatted string of a directory (for display/context).
/// Similar to the `tree` command. Skips SKIP_DIRS.
pub fn format_tree(dir: &Path, max_depth: usize) -> String {
    let mut output = String::new();
    format_tree_recursive(dir, "", 0, max_depth, &mut output);
    output
}

fn format_tree_recursive(dir: &Path, prefix: &str, depth: usize, max_depth: usize, output: &mut String) {
    if depth >= max_depth {
        return;
    }
    let Ok(mut entries): Result<Vec<_>, _> = std::fs::read_dir(dir).map(|rd| rd.filter_map(|e| e.ok()).collect()) else {
        return;
    };
    entries.sort_by_key(|e| e.file_name());

    for (i, entry) in entries.iter().enumerate() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with('.') || SKIP_DIRS.contains(&name_str.as_ref()) {
            continue;
        }

        let is_last = i == entries.len() - 1;
        let connector = if is_last { "└── " } else { "├── " };
        let child_prefix = if is_last { "    " } else { "│   " };

        output.push_str(prefix);
        output.push_str(connector);
        output.push_str(&name_str);
        output.push('\n');

        if entry.path().is_dir() {
            format_tree_recursive(
                &entry.path(),
                &format!("{}{}", prefix, child_prefix),
                depth + 1,
                max_depth,
                output,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn collects_files_by_extension() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/app.ts"), "code").unwrap();
        fs::write(dir.path().join("src/style.css"), "css").unwrap();
        fs::write(dir.path().join("src/readme.md"), "docs").unwrap();

        let ts_files = collect_files_by_ext(dir.path(), &["ts"]);
        assert_eq!(ts_files.len(), 1);

        let all_code = collect_files_by_ext(dir.path(), &["ts", "css"]);
        assert_eq!(all_code.len(), 2);
    }

    #[test]
    fn skips_node_modules() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("node_modules/pkg")).unwrap();
        fs::write(dir.path().join("node_modules/pkg/index.js"), "code").unwrap();
        fs::write(dir.path().join("app.ts"), "code").unwrap();

        let files = collect_files_by_ext(dir.path(), &["ts", "js"]);
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("app.ts"));
    }

    #[test]
    fn skips_hidden_dirs() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".git")).unwrap();
        fs::write(dir.path().join(".git/config"), "cfg").unwrap();
        fs::write(dir.path().join("app.ts"), "code").unwrap();

        let files = collect_all_files(dir.path());
        assert_eq!(files.len(), 1);
    }
}
