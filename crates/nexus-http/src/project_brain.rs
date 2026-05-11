//! Project Brain — self-indexing memory that persists across agent sessions.
//!
//! The brain scans the project, builds a structural index, and stores
//! decisions/patterns for future sessions.

use std::path::Path;
use serde::{Deserialize, Serialize};

/// A snapshot of the project structure and key patterns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectBrain {
    /// Project tech stack detection
    pub stack: TechStack,
    /// File structure summary (top-level dirs + key files)
    pub structure: String,
    /// Detected patterns and conventions
    pub patterns: Vec<String>,
    /// Past decisions the agent made (persisted)
    pub decisions: Vec<Decision>,
    /// Key files the agent should know about
    pub key_files: Vec<KeyFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TechStack {
    pub language: String,
    pub framework: String,
    pub package_manager: String,
    pub test_framework: String,
    pub has_typescript: bool,
    pub has_docker: bool,
    pub has_ci: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub timestamp: String,
    pub description: String,
    pub context: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyFile {
    pub path: String,
    pub role: String, // "config", "entry", "router", "schema", "test", "style"
}

impl ProjectBrain {
    /// Scan a project directory and build the brain.
    pub fn scan(project_dir: &Path) -> Self {
        let stack = detect_stack(project_dir);
        let structure = build_structure_summary(project_dir);
        let patterns = detect_patterns(project_dir, &stack);
        let key_files = find_key_files(project_dir);

        Self {
            stack,
            structure,
            patterns,
            decisions: Vec::new(),
            key_files,
        }
    }

    /// Convert to a context string for the LLM system prompt.
    pub fn to_context(&self) -> String {
        let mut ctx = String::new();

        ctx.push_str("## PROJECT CONTEXT (auto-indexed)\n\n");

        ctx.push_str(&format!(
            "**Stack**: {} / {} / {}\n",
            self.stack.language, self.stack.framework, self.stack.package_manager
        ));
        if !self.stack.test_framework.is_empty() {
            ctx.push_str(&format!("**Tests**: {}\n", self.stack.test_framework));
        }
        ctx.push('\n');

        ctx.push_str("**Structure**:\n```\n");
        ctx.push_str(&self.structure);
        ctx.push_str("\n```\n\n");

        if !self.patterns.is_empty() {
            ctx.push_str("**Conventions**:\n");
            for p in &self.patterns {
                ctx.push_str(&format!("- {}\n", p));
            }
            ctx.push('\n');
        }

        if !self.key_files.is_empty() {
            ctx.push_str("**Key files**:\n");
            for f in &self.key_files {
                ctx.push_str(&format!("- `{}` — {}\n", f.path, f.role));
            }
            ctx.push('\n');
        }

        if !self.decisions.is_empty() {
            ctx.push_str("**Past decisions**:\n");
            for d in self.decisions.iter().rev().take(10) {
                ctx.push_str(&format!("- {}: {}\n", d.timestamp, d.description));
            }
        }

        ctx
    }

    /// Load brain from a .nexus/brain.json file, or scan fresh.
    pub fn load_or_scan(project_dir: &Path) -> Self {
        let brain_path = project_dir.join(".nexus").join("brain.json");
        if brain_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&brain_path) {
                if let Ok(brain) = serde_json::from_str::<ProjectBrain>(&content) {
                    return brain;
                }
            }
        }
        let brain = Self::scan(project_dir);
        brain.save(project_dir);
        brain
    }

    /// Save brain to .nexus/brain.json
    pub fn save(&self, project_dir: &Path) {
        let nexus_dir = project_dir.join(".nexus");
        let _ = std::fs::create_dir_all(&nexus_dir);
        let brain_path = nexus_dir.join("brain.json");
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(brain_path, json);
        }
    }

    /// Record a decision for future sessions.
    pub fn record_decision(&mut self, description: &str, context: &str) {
        self.decisions.push(Decision {
            timestamp: chrono::Utc::now().to_rfc3339(),
            description: description.to_string(),
            context: context.to_string(),
        });
    }
}

fn detect_stack(dir: &Path) -> TechStack {
    let has_pkg_json = dir.join("package.json").exists();
    let has_cargo = dir.join("Cargo.toml").exists();
    let has_requirements = dir.join("requirements.txt").exists();
    let has_go_mod = dir.join("go.mod").exists();
    let has_tsconfig = dir.join("tsconfig.json").exists();
    let has_next_config = dir.join("next.config.js").exists()
        || dir.join("next.config.mjs").exists()
        || dir.join("next.config.ts").exists();
    let has_docker =
        dir.join("Dockerfile").exists() || dir.join("docker-compose.yml").exists();
    let has_ci = dir.join(".github").join("workflows").exists();

    let (language, framework, pkg_mgr) = if has_next_config {
        (
            "TypeScript/JavaScript",
            "Next.js",
            if dir.join("pnpm-lock.yaml").exists() {
                "pnpm"
            } else if dir.join("yarn.lock").exists() {
                "yarn"
            } else {
                "npm"
            },
        )
    } else if has_pkg_json {
        let ts = if has_tsconfig {
            "TypeScript"
        } else {
            "JavaScript"
        };
        // Read package.json to detect framework
        let framework =
            if let Ok(pkg) = std::fs::read_to_string(dir.join("package.json")) {
                if pkg.contains("\"react\"") {
                    "React"
                } else if pkg.contains("\"vue\"") {
                    "Vue"
                } else if pkg.contains("\"express\"") {
                    "Express"
                } else if pkg.contains("\"fastify\"") {
                    "Fastify"
                } else {
                    "Node.js"
                }
            } else {
                "Node.js"
            };
        (ts, framework, "npm")
    } else if has_cargo {
        ("Rust", "Cargo", "cargo")
    } else if has_requirements {
        let framework =
            if let Ok(req) = std::fs::read_to_string(dir.join("requirements.txt")) {
                if req.contains("django") {
                    "Django"
                } else if req.contains("flask") {
                    "Flask"
                } else if req.contains("fastapi") {
                    "FastAPI"
                } else {
                    "Python"
                }
            } else {
                "Python"
            };
        ("Python", framework, "pip")
    } else if has_go_mod {
        ("Go", "Go Modules", "go")
    } else {
        ("Unknown", "Unknown", "unknown")
    };

    let test_framework = if has_pkg_json {
        if let Ok(pkg) = std::fs::read_to_string(dir.join("package.json")) {
            if pkg.contains("vitest") {
                "Vitest"
            } else if pkg.contains("jest") {
                "Jest"
            } else if pkg.contains("mocha") {
                "Mocha"
            } else if pkg.contains("playwright") {
                "Playwright"
            } else {
                ""
            }
        } else {
            ""
        }
    } else if has_cargo {
        "cargo test"
    } else if has_requirements {
        "pytest"
    } else {
        ""
    };

    TechStack {
        language: language.to_string(),
        framework: framework.to_string(),
        package_manager: pkg_mgr.to_string(),
        test_framework: test_framework.to_string(),
        has_typescript: has_tsconfig,
        has_docker,
        has_ci,
    }
}

fn build_structure_summary(dir: &Path) -> String {
    let mut output = String::new();
    fn walk(dir: &Path, prefix: &str, depth: usize, output: &mut String) {
        if depth > 2 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') && name != ".env.example" {
                continue;
            }
            if matches!(
                name.as_str(),
                "node_modules"
                    | ".next"
                    | "target"
                    | "__pycache__"
                    | ".git"
                    | "dist"
                    | "build"
            ) {
                continue;
            }
            let is_dir = entry.path().is_dir();
            output.push_str(&format!(
                "{}{}\n",
                prefix,
                if is_dir {
                    format!("{}/", name)
                } else {
                    name
                }
            ));
            if is_dir {
                walk(
                    &entry.path(),
                    &format!("{}  ", prefix),
                    depth + 1,
                    output,
                );
            }
        }
    }
    walk(dir, "", 0, &mut output);
    output
}

fn detect_patterns(dir: &Path, stack: &TechStack) -> Vec<String> {
    let mut patterns = Vec::new();

    // Check for src directory structure
    if dir.join("src").exists() {
        if dir.join("src/app").exists() {
            patterns.push("Next.js App Router pattern (src/app/)".to_string());
        }
        if dir.join("src/pages").exists() {
            patterns.push("Pages Router pattern (src/pages/)".to_string());
        }
        if dir.join("src/components").exists() {
            patterns.push("Component-based architecture (src/components/)".to_string());
        }
        if dir.join("src/lib").exists() {
            patterns.push("Shared utilities in src/lib/".to_string());
        }
        if dir.join("src/hooks").exists() {
            patterns.push("Custom hooks pattern (src/hooks/)".to_string());
        }
        if dir.join("src/api").exists() {
            patterns.push("API client layer (src/api/)".to_string());
        }
    }

    // Check for common config patterns
    if dir.join("tailwind.config.ts").exists()
        || dir.join("tailwind.config.js").exists()
    {
        patterns.push("Tailwind CSS for styling".to_string());
    }
    if dir.join(".eslintrc.json").exists() || dir.join("eslint.config.js").exists() {
        patterns.push("ESLint for linting".to_string());
    }
    if dir.join(".prettierrc").exists() || dir.join(".prettierrc.json").exists() {
        patterns.push("Prettier for code formatting".to_string());
    }

    // Check for environment files
    if dir.join(".env.example").exists() || dir.join(".env.local").exists() {
        patterns.push("Environment variables via .env files".to_string());
    }

    // TypeScript strictness
    if stack.has_typescript {
        if let Ok(tsconfig) = std::fs::read_to_string(dir.join("tsconfig.json")) {
            if tsconfig.contains("\"strict\": true")
                || tsconfig.contains("\"strict\":true")
            {
                patterns.push("TypeScript strict mode enabled".to_string());
            }
        }
    }

    patterns
}

fn find_key_files(dir: &Path) -> Vec<KeyFile> {
    let candidates = vec![
        ("package.json", "Package manifest and scripts"),
        ("tsconfig.json", "TypeScript configuration"),
        ("next.config.js", "Next.js configuration"),
        ("next.config.mjs", "Next.js configuration"),
        ("next.config.ts", "Next.js configuration"),
        ("tailwind.config.ts", "Tailwind CSS configuration"),
        ("tailwind.config.js", "Tailwind CSS configuration"),
        ("Cargo.toml", "Rust workspace configuration"),
        ("Dockerfile", "Docker build configuration"),
        ("docker-compose.yml", "Docker Compose services"),
        (".env.example", "Environment variable template"),
        ("src/app/layout.tsx", "Root layout component"),
        ("src/app/page.tsx", "Home page"),
        ("src/lib/utils.ts", "Shared utilities"),
        ("src/middleware.ts", "Next.js middleware"),
        ("prisma/schema.prisma", "Database schema"),
        ("drizzle.config.ts", "Drizzle ORM configuration"),
    ];

    let mut key_files = Vec::new();
    for (path, role) in candidates {
        if dir.join(path).exists() {
            key_files.push(KeyFile {
                path: path.to_string(),
                role: role.to_string(),
            });
        }
    }
    key_files
}

// ---------------------------------------------------------------------------
// Persistent Intelligence — each project gets smarter over time
// ---------------------------------------------------------------------------

/// Extended project intelligence that accumulates across sessions.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectIntelligence {
    /// Architecture decisions made and their outcomes.
    pub architecture_history: Vec<ArchDecisionRecord>,
    /// Patterns that worked well in this project.
    pub successful_patterns: Vec<String>,
    /// Failures and their fixes (to avoid repeating).
    pub failure_fixes: Vec<FailureFix>,
    /// Taste score history over time.
    pub taste_history: Vec<(String, u32)>, // (timestamp, score)
    /// User preferences specific to this project.
    pub user_preferences: Vec<(String, String)>, // (key, value)
    /// Total mutations applied.
    pub total_mutations: u32,
    /// Total autonomous improvements.
    pub total_autonomous_runs: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchDecisionRecord {
    pub area: String,
    pub choice: String,
    pub reason: String,
    pub outcome: String, // "success", "override", "failure"
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureFix {
    pub error: String,
    pub fix_applied: String,
    pub worked: bool,
    pub timestamp: String,
}

impl ProjectIntelligence {
    /// Load project intelligence from disk.
    pub fn load(project_dir: &Path) -> Self {
        let path = project_dir.join(".nexus").join("intelligence.json");
        if path.exists() {
            if let Ok(data) = std::fs::read_to_string(&path) {
                if let Ok(intel) = serde_json::from_str(&data) {
                    return intel;
                }
            }
        }
        Self::default()
    }

    /// Save project intelligence to disk.
    pub fn save(&self, project_dir: &Path) {
        let nexus_dir = project_dir.join(".nexus");
        let _ = std::fs::create_dir_all(&nexus_dir);
        let path = nexus_dir.join("intelligence.json");
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    /// Record an architecture decision.
    pub fn record_decision(&mut self, area: &str, choice: &str, reason: &str, outcome: &str) {
        self.architecture_history.push(ArchDecisionRecord {
            area: area.into(),
            choice: choice.into(),
            reason: reason.into(),
            outcome: outcome.into(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
        // Keep last 50 decisions
        if self.architecture_history.len() > 50 {
            self.architecture_history.drain(0..self.architecture_history.len() - 50);
        }
    }

    /// Record a taste score.
    pub fn record_taste_score(&mut self, score: u32) {
        self.taste_history.push((
            chrono::Utc::now().to_rfc3339(),
            score,
        ));
        if self.taste_history.len() > 100 {
            self.taste_history.drain(0..self.taste_history.len() - 100);
        }
    }

    /// Record a failure and its fix.
    pub fn record_failure_fix(&mut self, error: &str, fix: &str, worked: bool) {
        self.failure_fixes.push(FailureFix {
            error: error.into(),
            fix_applied: fix.into(),
            worked,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
    }

    /// Record a successful pattern.
    pub fn record_successful_pattern(&mut self, pattern: &str) {
        if !self.successful_patterns.contains(&pattern.to_string()) {
            self.successful_patterns.push(pattern.into());
        }
        // Keep last 30
        if self.successful_patterns.len() > 30 {
            self.successful_patterns.drain(0..self.successful_patterns.len() - 30);
        }
    }

    /// Convert to context string for LLM prompts.
    pub fn to_context(&self) -> String {
        let mut ctx = String::new();

        if !self.architecture_history.is_empty() {
            ctx.push_str("## Project Architecture History\n");
            for d in self.architecture_history.iter().rev().take(10) {
                ctx.push_str(&format!(
                    "- {}: {} ({})\n",
                    d.area, d.choice, d.outcome
                ));
            }
        }

        if !self.successful_patterns.is_empty() {
            ctx.push_str("\n## Successful Patterns (reuse these)\n");
            for p in &self.successful_patterns {
                ctx.push_str(&format!("- {}\n", p));
            }
        }

        if !self.failure_fixes.is_empty() {
            ctx.push_str("\n## Known Issues and Fixes (avoid repeating)\n");
            for f in self.failure_fixes.iter().rev().take(5) {
                ctx.push_str(&format!(
                    "- Error: {} → Fix: {} ({})\n",
                    f.error,
                    f.fix_applied,
                    if f.worked { "worked" } else { "didn't work" }
                ));
            }
        }

        if !self.taste_history.is_empty() {
            let Some(latest_entry) = self.taste_history.last() else { return ctx; };
            let latest = latest_entry.1;
            let trend = if self.taste_history.len() >= 2 {
                let prev = self.taste_history[self.taste_history.len() - 2].1;
                if latest > prev { "improving" } else if latest < prev { "declining" } else { "stable" }
            } else {
                "initial"
            };
            ctx.push_str(&format!("\n## Quality: {}/100 ({})\n", latest, trend));
        }

        ctx
    }
}
