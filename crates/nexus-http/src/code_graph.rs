//! Live Semantic Code Graph — AST-like analysis of project structure.
//! Builds a dependency graph showing imports, exports, function calls, and coupling.

use std::path::{Path, PathBuf};
use std::collections::{HashMap, HashSet};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeGraph {
    pub files: Vec<FileNode>,
    pub edges: Vec<DependencyEdge>,
    pub symbols: Vec<Symbol>,
    pub stats: GraphStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileNode {
    pub path: String,
    pub language: String,
    pub lines: usize,
    pub imports: Vec<String>,       // files this file imports from
    pub exported_symbols: Vec<String>, // what this file exports
    pub complexity: String,          // low|medium|high
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyEdge {
    pub from: String,   // source file
    pub to: String,     // target file
    pub edge_type: String,  // "import", "type_reference", "function_call"
    pub symbols: Vec<String>,  // what's being imported
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    pub name: String,
    pub kind: String,       // "function", "class", "type", "interface", "component", "const", "enum"
    pub file: String,
    pub line: usize,
    pub exported: bool,
    pub used_by: Vec<String>,  // files that use this symbol
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphStats {
    pub total_files: usize,
    pub total_lines: usize,
    pub total_symbols: usize,
    pub total_edges: usize,
    pub most_imported: Vec<(String, usize)>,  // (file, import_count) top 10
    pub most_complex: Vec<(String, usize)>,   // (file, lines) top 10
    pub orphan_files: Vec<String>,             // files with no imports/exports
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactAnalysis {
    pub file: String,
    pub direct_dependents: Vec<String>,
    pub transitive_dependents: Vec<String>,
    pub risk_level: String,
    pub suggestion: String,
}

impl CodeGraph {
    pub fn build(project_dir: &Path) -> Self {
        let mut files = Vec::new();
        let mut edges = Vec::new();
        let mut symbols = Vec::new();
        let mut import_map: HashMap<String, Vec<String>> = HashMap::new();

        // Walk source files
        let source_files = collect_source_files(project_dir);

        for file_path in &source_files {
            let rel_path = file_path.strip_prefix(project_dir)
                .unwrap_or(file_path)
                .display()
                .to_string();

            let content = match std::fs::read_to_string(file_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let lang = detect_language(file_path);
            let lines = content.lines().count();
            let (file_imports, file_exports, file_symbols) = analyze_file(&content, &rel_path, &lang);

            // Track imports for edge building
            import_map.insert(rel_path.clone(), file_imports.clone());

            // Build symbols
            for sym in &file_symbols {
                symbols.push(sym.clone());
            }

            let complexity = if lines > 500 { "high" } else if lines > 200 { "medium" } else { "low" };

            files.push(FileNode {
                path: rel_path.clone(),
                language: lang,
                lines,
                imports: file_imports,
                exported_symbols: file_exports,
                complexity: complexity.to_string(),
            });
        }

        // Build edges from import map
        for (from_file, imports) in &import_map {
            for import_path in imports {
                // Resolve import to actual file
                let resolved = resolve_import(import_path, from_file, &files);
                if let Some(to_file) = resolved {
                    edges.push(DependencyEdge {
                        from: from_file.clone(),
                        to: to_file,
                        edge_type: "import".to_string(),
                        symbols: vec![],
                    });
                }
            }
        }

        // Build "used_by" for symbols
        for sym in &mut symbols {
            for (file, imports) in &import_map {
                if imports.iter().any(|i| i.contains(&sym.name)) && file != &sym.file {
                    sym.used_by.push(file.clone());
                }
            }
        }

        // Compute stats
        let mut import_counts: HashMap<String, usize> = HashMap::new();
        for edge in &edges {
            *import_counts.entry(edge.to.clone()).or_default() += 1;
        }
        let mut most_imported: Vec<(String, usize)> = import_counts.into_iter().collect();
        most_imported.sort_by(|a, b| b.1.cmp(&a.1));

        let mut most_complex: Vec<(String, usize)> = files.iter().map(|f| (f.path.clone(), f.lines)).collect();
        most_complex.sort_by(|a, b| b.1.cmp(&a.1));

        let connected: HashSet<String> = edges.iter()
            .flat_map(|e| vec![e.from.clone(), e.to.clone()])
            .collect();
        let orphans: Vec<String> = files.iter()
            .filter(|f| !connected.contains(&f.path))
            .map(|f| f.path.clone())
            .collect();

        let stats = GraphStats {
            total_files: files.len(),
            total_lines: files.iter().map(|f| f.lines).sum(),
            total_symbols: symbols.len(),
            total_edges: edges.len(),
            most_imported: most_imported.into_iter().take(10).collect(),
            most_complex: most_complex.into_iter().take(10).collect(),
            orphan_files: orphans,
        };

        CodeGraph { files, edges, symbols, stats }
    }

    /// Analyze impact: "What breaks if I change this file?"
    pub fn impact_analysis(&self, file_path: &str) -> ImpactAnalysis {
        // Direct dependents
        let direct: Vec<String> = self.edges.iter()
            .filter(|e| e.to == file_path)
            .map(|e| e.from.clone())
            .collect();

        // Transitive dependents (BFS)
        let mut visited = HashSet::new();
        let mut queue: Vec<String> = direct.clone();
        visited.insert(file_path.to_string());

        while let Some(current) = queue.pop() {
            if visited.contains(&current) { continue; }
            visited.insert(current.clone());
            let deps: Vec<String> = self.edges.iter()
                .filter(|e| e.to == current)
                .map(|e| e.from.clone())
                .collect();
            queue.extend(deps);
        }
        visited.remove(file_path);
        let transitive: Vec<String> = visited.into_iter()
            .filter(|f| !direct.contains(f))
            .collect();

        let total = direct.len() + transitive.len();
        let risk = if total > 10 { "critical" } else if total > 5 { "high" } else if total > 2 { "medium" } else { "low" };

        let suggestion = if total == 0 {
            "This file has no dependents. Safe to modify freely.".to_string()
        } else {
            format!("{} files depend on this. Test all dependents after changes.", total)
        };

        ImpactAnalysis {
            file: file_path.to_string(),
            direct_dependents: direct,
            transitive_dependents: transitive,
            risk_level: risk.to_string(),
            suggestion,
        }
    }

    /// Find all usages of a symbol
    pub fn find_usages(&self, symbol_name: &str) -> Vec<Symbol> {
        self.symbols.iter()
            .filter(|s| s.name == symbol_name)
            .cloned()
            .collect()
    }

    /// Cache to file
    pub fn save(&self, project_dir: &Path) {
        let cache_dir = project_dir.join(".nexus");
        let _ = std::fs::create_dir_all(&cache_dir);
        if let Ok(json) = serde_json::to_string(self) {
            let _ = std::fs::write(cache_dir.join("code_graph.json"), json);
        }
    }

    pub fn load_or_build(project_dir: &Path) -> Self {
        let cache = project_dir.join(".nexus").join("code_graph.json");
        if cache.exists() {
            if let Ok(content) = std::fs::read_to_string(&cache) {
                if let Ok(graph) = serde_json::from_str(&content) {
                    return graph;
                }
            }
        }
        let graph = Self::build(project_dir);
        graph.save(project_dir);
        graph
    }
}

// ---- Helpers ----

fn collect_source_files(dir: &Path) -> Vec<PathBuf> {
    crate::file_utils::collect_files_by_ext(
        dir,
        &["ts", "tsx", "js", "jsx", "rs", "py", "go", "java", "css", "scss",
          "json", "yaml", "yml", "toml", "sql", "prisma", "graphql"],
    )
}


fn detect_language(path: &Path) -> String {
    match path.extension().and_then(|e| e.to_str()) {
        Some("ts" | "tsx") => "typescript",
        Some("js" | "jsx") => "javascript",
        Some("rs") => "rust",
        Some("py") => "python",
        Some("go") => "go",
        Some("java") => "java",
        Some("css" | "scss") => "css",
        Some("json") => "json",
        Some("yaml" | "yml") => "yaml",
        Some("sql") => "sql",
        _ => "unknown",
    }.to_string()
}

fn analyze_file(content: &str, file_path: &str, lang: &str) -> (Vec<String>, Vec<String>, Vec<Symbol>) {
    let mut imports = Vec::new();
    let mut exports = Vec::new();
    let mut symbols = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        match lang {
            "typescript" | "javascript" => {
                // Imports
                if trimmed.starts_with("import ") {
                    if let Some(from_idx) = trimmed.find("from ") {
                        let module = trimmed[from_idx + 5..].trim().trim_matches(|c| c == '\'' || c == '"' || c == ';');
                        imports.push(module.to_string());
                    }
                }
                // Exports
                if trimmed.starts_with("export ") {
                    if let Some(name) = extract_export_name(trimmed) {
                        exports.push(name.clone());
                        let kind = if trimmed.contains("function") { "function" }
                            else if trimmed.contains("class") { "class" }
                            else if trimmed.contains("interface") { "interface" }
                            else if trimmed.contains("type ") { "type" }
                            else if trimmed.contains("enum") { "enum" }
                            else if trimmed.contains("const") { "const" }
                            else { "export" };
                        symbols.push(Symbol {
                            name, kind: kind.to_string(), file: file_path.to_string(),
                            line: line_num + 1, exported: true, used_by: Vec::new(),
                        });
                    }
                }
                // Function declarations
                if (trimmed.starts_with("function ") || trimmed.starts_with("async function ")) && !trimmed.starts_with("export") {
                    if let Some(name) = extract_function_name(trimmed) {
                        symbols.push(Symbol {
                            name, kind: "function".to_string(), file: file_path.to_string(),
                            line: line_num + 1, exported: false, used_by: Vec::new(),
                        });
                    }
                }
            }
            "rust" => {
                if trimmed.starts_with("use ") {
                    let module = trimmed.trim_start_matches("use ").trim_end_matches(';').to_string();
                    imports.push(module);
                }
                if trimmed.starts_with("pub fn ") || trimmed.starts_with("pub async fn ") {
                    if let Some(name) = extract_rust_fn_name(trimmed) {
                        exports.push(name.clone());
                        symbols.push(Symbol {
                            name, kind: "function".to_string(), file: file_path.to_string(),
                            line: line_num + 1, exported: true, used_by: Vec::new(),
                        });
                    }
                }
                if trimmed.starts_with("pub struct ") || trimmed.starts_with("pub enum ") {
                    let kind = if trimmed.contains("struct") { "struct" } else { "enum" };
                    if let Some(name) = extract_rust_type_name(trimmed) {
                        exports.push(name.clone());
                        symbols.push(Symbol {
                            name, kind: kind.to_string(), file: file_path.to_string(),
                            line: line_num + 1, exported: true, used_by: Vec::new(),
                        });
                    }
                }
            }
            "python" => {
                if trimmed.starts_with("import ") || trimmed.starts_with("from ") {
                    imports.push(trimmed.to_string());
                }
                if trimmed.starts_with("def ") || trimmed.starts_with("class ") {
                    let kind = if trimmed.starts_with("class") { "class" } else { "function" };
                    let name_end = trimmed.find('(').or_else(|| trimmed.find(':')).unwrap_or(trimmed.len());
                    let prefix_len = if kind == "class" { 6 } else { 4 };
                    if name_end > prefix_len {
                        let name = trimmed[prefix_len..name_end].trim().to_string();
                        exports.push(name.clone());
                        symbols.push(Symbol {
                            name, kind: kind.to_string(), file: file_path.to_string(),
                            line: line_num + 1, exported: true, used_by: Vec::new(),
                        });
                    }
                }
            }
            _ => {}
        }
    }

    (imports, exports, symbols)
}

fn extract_export_name(line: &str) -> Option<String> {
    let rest = line.trim_start_matches("export ").trim_start_matches("default ");
    for prefix in &["function ", "async function ", "class ", "interface ", "type ", "enum ", "const ", "let ", "var "] {
        if let Some(stripped) = rest.strip_prefix(prefix) {
            let after = stripped.trim();
            let end = after.find(|c: char| !c.is_alphanumeric() && c != '_').unwrap_or(after.len());
            if end > 0 { return Some(after[..end].to_string()); }
        }
    }
    None
}

fn extract_function_name(line: &str) -> Option<String> {
    let rest = line.trim_start_matches("async ").trim_start_matches("function ");
    let end = rest.find('(').unwrap_or(rest.len());
    let name = rest[..end].trim();
    if !name.is_empty() { Some(name.to_string()) } else { None }
}

fn extract_rust_fn_name(line: &str) -> Option<String> {
    let rest = line.trim_start_matches("pub ").trim_start_matches("async ").trim_start_matches("fn ");
    let end = rest.find('(').or_else(|| rest.find('<')).unwrap_or(rest.len());
    let name = rest[..end].trim();
    if !name.is_empty() { Some(name.to_string()) } else { None }
}

fn extract_rust_type_name(line: &str) -> Option<String> {
    let rest = line.trim_start_matches("pub ").trim_start_matches("struct ").trim_start_matches("enum ");
    let end = rest.find(|c: char| !c.is_alphanumeric() && c != '_').unwrap_or(rest.len());
    if end > 0 { Some(rest[..end].to_string()) } else { None }
}

fn resolve_import(import_path: &str, _from_file: &str, files: &[FileNode]) -> Option<String> {
    // Handle relative imports
    if import_path.starts_with('.') || import_path.starts_with('@') {
        let clean = import_path.replace("@/", "src/");
        // Try common extensions
        for ext in &["", ".ts", ".tsx", ".js", ".jsx", "/index.ts", "/index.tsx", "/index.js"] {
            let candidate = format!("{}{}", clean, ext);
            if files.iter().any(|f| f.path == candidate) {
                return Some(candidate);
            }
        }
    }
    None
}
