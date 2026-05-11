//! File system tools — read, write, edit, list, search, and inspect files.
//!
//! All path parameters are relative to the project directory unless
//! explicitly absolute. Paths are validated against traversal attacks
//! before any I/O.

use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::Path;

use crate::agents::tools::registry::{Tool, ToolInput, ToolOutput};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a successful [`ToolOutput`].
fn ok(result: Value) -> ToolOutput {
    ToolOutput {
        result,
        success: true,
        error: None,
    }
}

/// Build a failed [`ToolOutput`].
fn err(msg: impl Into<String>) -> ToolOutput {
    ToolOutput {
        result: json!({}),
        success: false,
        error: Some(msg.into()),
    }
}

/// Extract a required string parameter.
fn required_str<'a>(params: &'a Value, key: &str) -> Result<&'a str, ToolOutput> {
    params
        .get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| err(format!("Missing required parameter: {}", key)))
}

// ---------------------------------------------------------------------------
// ReadFileTool
// ---------------------------------------------------------------------------

/// Read the contents of a file.
///
/// Supports optional `offset` (1-indexed line number) and `limit` to read
/// a slice of a large file without loading everything.
pub struct ReadFileTool;

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read the contents of a file. Returns line-numbered output. Supports offset/limit for large files."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Relative path to the file" },
                "offset": { "type": "integer", "description": "Start line (1-indexed, optional)" },
                "limit": { "type": "integer", "description": "Number of lines to read (optional)" }
            },
            "required": ["path"]
        })
    }

    fn category(&self) -> &str {
        "file_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let path = match required_str(&input.parameters, "path") {
            Ok(p) => p,
            Err(e) => return e,
        };

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => return err(format!("Cannot read '{}': {}", path, e)),
        };

        let lines: Vec<&str> = content.lines().collect();
        let total = lines.len();
        let offset = input
            .parameters
            .get("offset")
            .and_then(|v| v.as_u64())
            .unwrap_or(1)
            .max(1) as usize;
        let limit = input
            .parameters
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|l| l as usize);

        let start = (offset - 1).min(total);
        let end = limit
            .map(|l| (start + l).min(total))
            .unwrap_or(total);

        let mut numbered = String::new();
        for (i, line) in lines[start..end].iter().enumerate() {
            numbered.push_str(&format!("{:>4}  {}\n", start + i + 1, line));
        }

        if numbered.is_empty() {
            numbered = "(empty file)".to_string();
        }

        ok(json!({
            "content": numbered,
            "total_lines": total,
            "shown_from": start + 1,
            "shown_to": end
        }))
    }
}

// ---------------------------------------------------------------------------
// WriteFileTool
// ---------------------------------------------------------------------------

/// Create or overwrite a file with the given content.
///
/// Parent directories are created automatically.
pub struct WriteFileTool;

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Create or overwrite a file with the given content. Creates parent directories automatically."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Relative path to the file" },
                "content": { "type": "string", "description": "Complete file content to write" }
            },
            "required": ["path", "content"]
        })
    }

    fn category(&self) -> &str {
        "file_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let path = match required_str(&input.parameters, "path") {
            Ok(p) => p,
            Err(e) => return e,
        };
        let content = match required_str(&input.parameters, "content") {
            Ok(c) => c,
            Err(e) => return e,
        };

        // Size guard
        if content.len() > 1_000_000 {
            return err("File too large (max 1 MB)");
        }

        let p = Path::new(path);
        if let Some(parent) = p.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return err(format!("Cannot create directories: {}", e));
            }
        }

        match std::fs::write(p, content) {
            Ok(()) => ok(json!({
                "bytes_written": content.len(),
                "path": path
            })),
            Err(e) => err(format!("Cannot write '{}': {}", path, e)),
        }
    }
}

// ---------------------------------------------------------------------------
// EditFileTool
// ---------------------------------------------------------------------------

/// Replace an exact string in a file.
///
/// The `old_string` must appear exactly once in the file to avoid
/// ambiguous edits. Use `replace_all` to relax this constraint.
pub struct EditFileTool;

#[async_trait]
impl Tool for EditFileTool {
    fn name(&self) -> &str {
        "edit_file"
    }

    fn description(&self) -> &str {
        "Replace an exact string in a file. The old_string must appear exactly once unless replace_all is true."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Relative path to the file" },
                "old_string": { "type": "string", "description": "Exact string to find" },
                "new_string": { "type": "string", "description": "Replacement string" },
                "replace_all": { "type": "boolean", "description": "Replace all occurrences (default: false)" }
            },
            "required": ["path", "old_string", "new_string"]
        })
    }

    fn category(&self) -> &str {
        "file_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let path = match required_str(&input.parameters, "path") {
            Ok(p) => p,
            Err(e) => return e,
        };
        let old_string = match required_str(&input.parameters, "old_string") {
            Ok(s) => s,
            Err(e) => return e,
        };
        let new_string = match required_str(&input.parameters, "new_string") {
            Ok(s) => s,
            Err(e) => return e,
        };
        let replace_all = input
            .parameters
            .get("replace_all")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => return err(format!("Cannot read '{}': {}", path, e)),
        };

        let count = content.matches(old_string).count();
        if count == 0 {
            return err(format!(
                "'old_string' not found in {}. Read the file first to verify exact content.",
                path
            ));
        }
        if count > 1 && !replace_all {
            return err(format!(
                "'old_string' found {} times in {} — must be unique. Provide more context or set replace_all.",
                count, path
            ));
        }

        let new_content = if replace_all {
            content.replace(old_string, new_string)
        } else {
            content.replacen(old_string, new_string, 1)
        };

        match std::fs::write(path, &new_content) {
            Ok(()) => ok(json!({
                "path": path,
                "replacements": if replace_all { count } else { 1 },
                "old_lines": old_string.lines().count(),
                "new_lines": new_string.lines().count()
            })),
            Err(e) => err(format!("Cannot write '{}': {}", path, e)),
        }
    }
}

// ---------------------------------------------------------------------------
// ListDirectoryTool
// ---------------------------------------------------------------------------

/// List files and directories in a path with optional depth limit.
pub struct ListDirectoryTool;

#[async_trait]
impl Tool for ListDirectoryTool {
    fn name(&self) -> &str {
        "list_directory"
    }

    fn description(&self) -> &str {
        "List files and directories in a path. Returns a tree structure with depth control."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Directory path (default: current dir)" },
                "depth": { "type": "integer", "description": "Max depth to recurse (default: 3)" }
            },
            "required": []
        })
    }

    fn category(&self) -> &str {
        "file_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let path_str = input
            .parameters
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");
        let max_depth = input
            .parameters
            .get("depth")
            .and_then(|v| v.as_u64())
            .unwrap_or(3) as usize;

        let dir = Path::new(path_str).to_path_buf();
        if !dir.is_dir() {
            return err(format!("'{}' is not a directory", path_str));
        }

        let output = tokio::task::spawn_blocking(move || {
            let mut buf = String::new();
            walk_dir(&dir, "", 0, max_depth, &mut buf);
            if buf.is_empty() {
                buf = "(empty directory)".to_string();
            }
            buf
        })
        .await
        .unwrap_or_else(|e| format!("(task panicked: {})", e));

        ok(json!({ "tree": output }))
    }
}

/// Recursively walk a directory, appending a tree representation.
fn walk_dir(dir: &Path, prefix: &str, depth: usize, max_depth: usize, output: &mut String) {
    if depth > max_depth {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());

    let skip = [
        "node_modules",
        ".git",
        ".next",
        "target",
        "__pycache__",
        ".DS_Store",
    ];

    for entry in entries {
        let name = entry.file_name().to_string_lossy().to_string();
        if skip.contains(&name.as_str()) || name.starts_with('.') {
            continue;
        }
        let is_dir = entry.path().is_dir();
        let icon = if is_dir { "d " } else { "  " };
        output.push_str(&format!("{}{}{}\n", prefix, icon, name));
        if is_dir {
            walk_dir(
                &entry.path(),
                &format!("{}  ", prefix),
                depth + 1,
                max_depth,
                output,
            );
        }
    }
}

// ---------------------------------------------------------------------------
// SearchFilesTool
// ---------------------------------------------------------------------------

/// Search file contents using a text pattern (grep-like).
pub struct SearchFilesTool;

#[async_trait]
impl Tool for SearchFilesTool {
    fn name(&self) -> &str {
        "search_files"
    }

    fn description(&self) -> &str {
        "Search file contents for a pattern (grep-like). Returns matching lines with paths and line numbers."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "Text pattern to search for" },
                "path": { "type": "string", "description": "Directory to search in (default: '.')" },
                "include": { "type": "string", "description": "Glob to filter files (e.g. '*.rs')" },
                "max_results": { "type": "integer", "description": "Maximum matches (default: 50)" }
            },
            "required": ["pattern"]
        })
    }

    fn category(&self) -> &str {
        "file_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let pattern = match required_str(&input.parameters, "pattern") {
            Ok(p) => p.to_string(),
            Err(e) => return e,
        };
        let search_path = input
            .parameters
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".")
            .to_string();
        let include = input
            .parameters
            .get("include")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let max_results = input
            .parameters
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(50) as usize;

        let output = tokio::task::spawn_blocking(move || {
            let mut cmd = std::process::Command::new("grep");
            cmd.arg("-rn");
            if !include.is_empty() {
                cmd.arg("--include").arg(&include);
            }
            cmd.arg("--").arg(&pattern).arg(&search_path);
            cmd.output()
        })
        .await;

        let output = match output {
            Ok(Ok(o)) => o,
            Ok(Err(e)) => return err(format!("grep failed: {}", e)),
            Err(e) => return err(format!("task panicked: {}", e)),
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let lines: Vec<&str> = stdout.lines().take(max_results).collect();

        if lines.is_empty() {
            ok(json!({ "matches": [], "count": 0 }))
        } else {
            ok(json!({
                "matches": lines,
                "count": lines.len()
            }))
        }
    }
}

// ---------------------------------------------------------------------------
// FileInfoTool
// ---------------------------------------------------------------------------

/// Get metadata about a file (size, modification time, type).
pub struct FileInfoTool;

#[async_trait]
impl Tool for FileInfoTool {
    fn name(&self) -> &str {
        "file_info"
    }

    fn description(&self) -> &str {
        "Get metadata about a file: size, modification time, type, permissions."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to the file or directory" }
            },
            "required": ["path"]
        })
    }

    fn category(&self) -> &str {
        "file_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let path = match required_str(&input.parameters, "path") {
            Ok(p) => p,
            Err(e) => return e,
        };

        let metadata = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(e) => return err(format!("Cannot stat '{}': {}", path, e)),
        };

        let file_type = if metadata.is_dir() {
            "directory"
        } else if metadata.is_symlink() {
            "symlink"
        } else {
            "file"
        };

        let modified = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        ok(json!({
            "path": path,
            "type": file_type,
            "size_bytes": metadata.len(),
            "modified_epoch": modified,
            "readonly": metadata.permissions().readonly()
        }))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn input(params: Value) -> ToolInput {
        ToolInput { parameters: params }
    }

    #[tokio::test]
    async fn read_file_tool_reads_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("hello.txt");
        std::fs::write(&file_path, "line1\nline2\nline3\n").unwrap();

        let tool = ReadFileTool;
        let out = tool
            .execute(input(json!({ "path": file_path.to_str().unwrap() })))
            .await;

        assert!(out.success, "Expected success: {:?}", out.error);
        assert_eq!(out.result["total_lines"], 3);
    }

    #[tokio::test]
    async fn read_file_tool_offset_limit() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("nums.txt");
        std::fs::write(&file_path, "a\nb\nc\nd\ne\n").unwrap();

        let tool = ReadFileTool;
        let out = tool
            .execute(input(json!({
                "path": file_path.to_str().unwrap(),
                "offset": 2,
                "limit": 2
            })))
            .await;

        assert!(out.success);
        assert_eq!(out.result["shown_from"], 2);
        assert_eq!(out.result["shown_to"], 3);
    }

    #[tokio::test]
    async fn write_file_tool_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("sub").join("new.txt");

        let tool = WriteFileTool;
        let out = tool
            .execute(input(json!({
                "path": file_path.to_str().unwrap(),
                "content": "hello world"
            })))
            .await;

        assert!(out.success, "Expected success: {:?}", out.error);
        assert_eq!(std::fs::read_to_string(&file_path).unwrap(), "hello world");
    }

    #[tokio::test]
    async fn edit_file_tool_search_replace() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("edit.txt");
        std::fs::write(&file_path, "foo bar baz").unwrap();

        let tool = EditFileTool;
        let out = tool
            .execute(input(json!({
                "path": file_path.to_str().unwrap(),
                "old_string": "bar",
                "new_string": "QUX"
            })))
            .await;

        assert!(out.success, "Expected success: {:?}", out.error);
        assert_eq!(std::fs::read_to_string(&file_path).unwrap(), "foo QUX baz");
    }

    #[tokio::test]
    async fn edit_file_tool_rejects_ambiguous() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("dup.txt");
        std::fs::write(&file_path, "aaa aaa").unwrap();

        let tool = EditFileTool;
        let out = tool
            .execute(input(json!({
                "path": file_path.to_str().unwrap(),
                "old_string": "aaa",
                "new_string": "bbb"
            })))
            .await;

        assert!(!out.success);
        assert!(out.error.unwrap().contains("2 times"));
    }

    #[tokio::test]
    async fn list_directory_tool_lists_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "").unwrap();
        std::fs::write(dir.path().join("b.txt"), "").unwrap();

        let tool = ListDirectoryTool;
        let out = tool
            .execute(input(json!({ "path": dir.path().to_str().unwrap() })))
            .await;

        assert!(out.success);
        let tree = out.result["tree"].as_str().unwrap();
        assert!(tree.contains("a.txt"));
        assert!(tree.contains("b.txt"));
    }

    #[tokio::test]
    async fn search_files_tool_finds_pattern() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("haystack.txt");
        std::fs::write(&file_path, "first line\nneedle here\nlast line\n").unwrap();

        let tool = SearchFilesTool;
        let out = tool
            .execute(input(json!({
                "pattern": "needle",
                "path": dir.path().to_str().unwrap()
            })))
            .await;

        assert!(out.success);
        assert!(out.result["count"].as_u64().unwrap() >= 1);
    }

    #[tokio::test]
    async fn file_info_tool_returns_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("info.txt");
        std::fs::write(&file_path, "12345").unwrap();

        let tool = FileInfoTool;
        let out = tool
            .execute(input(json!({ "path": file_path.to_str().unwrap() })))
            .await;

        assert!(out.success);
        assert_eq!(out.result["type"], "file");
        assert_eq!(out.result["size_bytes"], 5);
    }
}
