//! DevOps tools — Docker, system info, env management, Dockerfile generation.
//!
//! These tools help agents manage containers, inspect system resources,
//! and work with environment configuration.

use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::Path;

use crate::agents::tools::registry::{Tool, ToolInput, ToolOutput};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn ok(result: Value) -> ToolOutput {
    ToolOutput {
        result,
        success: true,
        error: None,
    }
}

fn err(msg: impl Into<String>) -> ToolOutput {
    ToolOutput {
        result: json!({}),
        success: false,
        error: Some(msg.into()),
    }
}

fn check_binary(name: &str) -> bool {
    std::process::Command::new("which")
        .arg(name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// DockerBuildTool
// ---------------------------------------------------------------------------

/// Build a Docker image.
pub struct DockerBuildTool;

#[async_trait]
impl Tool for DockerBuildTool {
    fn name(&self) -> &str {
        "docker_build"
    }

    fn description(&self) -> &str {
        "Build a Docker image from a Dockerfile. Shells out to 'docker build'."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "context_path": { "type": "string", "description": "Build context directory (default: .)" },
                "dockerfile": { "type": "string", "description": "Path to Dockerfile (default: Dockerfile)" },
                "tag": { "type": "string", "description": "Image tag (e.g., myapp:latest)" },
                "build_args": {
                    "type": "object",
                    "description": "Build arguments as key-value pairs",
                    "additionalProperties": { "type": "string" }
                },
                "no_cache": { "type": "boolean", "description": "Build without cache (default: false)" }
            },
            "required": ["tag"]
        })
    }

    fn category(&self) -> &str {
        "build_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        if !check_binary("docker") {
            return err("Docker is not installed or not in PATH.");
        }

        let tag = match input.parameters.get("tag").and_then(|v| v.as_str()) {
            Some(t) => t.to_string(),
            None => return err("Missing required parameter: tag"),
        };
        let context = input.parameters.get("context_path").and_then(|v| v.as_str()).unwrap_or(".").to_string();
        let dockerfile = input.parameters.get("dockerfile").and_then(|v| v.as_str()).map(|s| s.to_string());
        let no_cache = input.parameters.get("no_cache").and_then(|v| v.as_bool()).unwrap_or(false);
        let build_args: Vec<(String, String)> = input
            .parameters
            .get("build_args")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| v.as_str().map(|val| (k.clone(), val.to_string())))
                    .collect()
            })
            .unwrap_or_default();

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(300),
            tokio::task::spawn_blocking(move || {
                let mut cmd = std::process::Command::new("docker");
                cmd.arg("build").arg("-t").arg(&tag);

                if let Some(ref df) = dockerfile {
                    cmd.arg("-f").arg(df);
                }
                if no_cache {
                    cmd.arg("--no-cache");
                }
                for (key, val) in &build_args {
                    cmd.arg("--build-arg").arg(format!("{key}={val}"));
                }
                cmd.arg(&context);
                cmd.output()
            }),
        )
        .await;

        match result {
            Err(_) => err("Docker build timed out after 300s"),
            Ok(Err(e)) => err(format!("Task panicked: {e}")),
            Ok(Ok(Err(e))) => err(format!("Failed to start docker: {e}")),
            Ok(Ok(Ok(output))) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let combined = format!("{stdout}{stderr}");
                let truncated = combined.len() > 50_000;
                let output_text = if truncated { combined[..50_000].to_string() } else { combined };

                ok(json!({
                    "success": output.status.success(),
                    "exit_code": output.status.code().unwrap_or(-1),
                    "output": output_text,
                    "truncated": truncated
                }))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// DockerRunTool
// ---------------------------------------------------------------------------

/// Run a Docker container.
pub struct DockerRunTool;

#[async_trait]
impl Tool for DockerRunTool {
    fn name(&self) -> &str {
        "docker_run"
    }

    fn description(&self) -> &str {
        "Run a Docker container from an image. Shells out to 'docker run'."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "image": { "type": "string", "description": "Docker image to run" },
                "command": { "type": "string", "description": "Command to run in the container (optional)" },
                "ports": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Port mappings (e.g., ['8080:80', '3000:3000'])"
                },
                "env": {
                    "type": "object",
                    "description": "Environment variables as key-value pairs",
                    "additionalProperties": { "type": "string" }
                },
                "volumes": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Volume mounts (e.g., ['./data:/data'])"
                },
                "name": { "type": "string", "description": "Container name (optional)" },
                "detach": { "type": "boolean", "description": "Run in background (default: false)" },
                "rm": { "type": "boolean", "description": "Remove container after exit (default: true)" }
            },
            "required": ["image"]
        })
    }

    fn category(&self) -> &str {
        "build_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        if !check_binary("docker") {
            return err("Docker is not installed or not in PATH.");
        }

        let image = match input.parameters.get("image").and_then(|v| v.as_str()) {
            Some(i) => i.to_string(),
            None => return err("Missing required parameter: image"),
        };
        let command = input.parameters.get("command").and_then(|v| v.as_str()).map(|s| s.to_string());
        let detach = input.parameters.get("detach").and_then(|v| v.as_bool()).unwrap_or(false);
        let rm = input.parameters.get("rm").and_then(|v| v.as_bool()).unwrap_or(true);
        let container_name = input.parameters.get("name").and_then(|v| v.as_str()).map(|s| s.to_string());
        let ports: Vec<String> = input
            .parameters
            .get("ports")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let env_vars: Vec<(String, String)> = input
            .parameters
            .get("env")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| v.as_str().map(|val| (k.clone(), val.to_string())))
                    .collect()
            })
            .unwrap_or_default();
        let volumes: Vec<String> = input
            .parameters
            .get("volumes")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        let timeout_secs = if detach { 30u64 } else { 120 };

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            tokio::task::spawn_blocking(move || {
                let mut cmd = std::process::Command::new("docker");
                cmd.arg("run");

                if detach {
                    cmd.arg("-d");
                }
                if rm {
                    cmd.arg("--rm");
                }
                if let Some(ref name) = container_name {
                    cmd.arg("--name").arg(name);
                }
                for port in &ports {
                    cmd.arg("-p").arg(port);
                }
                for (key, val) in &env_vars {
                    cmd.arg("-e").arg(format!("{key}={val}"));
                }
                for vol in &volumes {
                    cmd.arg("-v").arg(vol);
                }
                cmd.arg(&image);
                if let Some(ref c) = command {
                    cmd.arg("sh").arg("-c").arg(c);
                }
                cmd.output()
            }),
        )
        .await;

        match result {
            Err(_) => err(format!("Docker run timed out after {timeout_secs}s")),
            Ok(Err(e)) => err(format!("Task panicked: {e}")),
            Ok(Ok(Err(e))) => err(format!("Failed to start docker: {e}")),
            Ok(Ok(Ok(output))) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let combined = format!("{stdout}{stderr}");
                let truncated = combined.len() > 50_000;
                let output_text = if truncated { combined[..50_000].to_string() } else { combined };

                ok(json!({
                    "success": output.status.success(),
                    "exit_code": output.status.code().unwrap_or(-1),
                    "output": output_text,
                    "truncated": truncated,
                    "detached": detach,
                    "container_id": if detach { stdout.trim().to_string() } else { String::new() }
                }))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// SystemInfoTool
// ---------------------------------------------------------------------------

/// Return system resource usage information.
pub struct SystemInfoTool;

#[async_trait]
impl Tool for SystemInfoTool {
    fn name(&self) -> &str {
        "system_info"
    }

    fn description(&self) -> &str {
        "Get system information: OS, CPU count, memory usage, disk usage, and uptime."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {}
        })
    }

    fn category(&self) -> &str {
        "data_ops"
    }

    async fn execute(&self, _input: ToolInput) -> ToolOutput {
        let result = tokio::task::spawn_blocking(gather_system_info).await;

        match result {
            Ok(output) => output,
            Err(e) => err(format!("Task panicked: {e}")),
        }
    }
}

fn gather_system_info() -> ToolOutput {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    // CPU info
    let cpu_count = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(0);

    // Hostname
    let hostname = std::process::Command::new("hostname")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // Uptime
    let uptime = std::process::Command::new("uptime")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // Disk usage
    let disk_usage = std::process::Command::new("df")
        .args(["-h", "/"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // Memory (platform-specific)
    let memory = if os == "macos" {
        std::process::Command::new("vm_stat")
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|| "unknown".to_string())
    } else {
        std::process::Command::new("free")
            .arg("-h")
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|| "unknown".to_string())
    };

    ok(json!({
        "os": os,
        "arch": arch,
        "cpu_count": cpu_count,
        "hostname": hostname,
        "uptime": uptime,
        "disk_usage": disk_usage,
        "memory": memory
    }))
}

// ---------------------------------------------------------------------------
// EnvManageTool
// ---------------------------------------------------------------------------

/// Manage environment variables from .env files.
pub struct EnvManageTool;

#[async_trait]
impl Tool for EnvManageTool {
    fn name(&self) -> &str {
        "env_manage"
    }

    fn description(&self) -> &str {
        "Get, set, or list environment variables in a .env file. Does not expose secret values by default."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "description": "Action: get, set, list, or delete", "enum": ["get", "set", "list", "delete"] },
                "env_file": { "type": "string", "description": "Path to .env file (default: .env)" },
                "key": { "type": "string", "description": "Environment variable name (required for get/set/delete)" },
                "value": { "type": "string", "description": "Value to set (required for set action)" },
                "show_values": { "type": "boolean", "description": "Show actual values in list (default: false for security)" }
            },
            "required": ["action"]
        })
    }

    fn category(&self) -> &str {
        "data_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let action = match input.parameters.get("action").and_then(|v| v.as_str()) {
            Some(a) => a.to_string(),
            None => return err("Missing required parameter: action"),
        };
        let env_file = input
            .parameters
            .get("env_file")
            .and_then(|v| v.as_str())
            .unwrap_or(".env")
            .to_string();
        let key = input.parameters.get("key").and_then(|v| v.as_str()).map(|s| s.to_string());
        let value = input.parameters.get("value").and_then(|v| v.as_str()).map(|s| s.to_string());
        let show_values = input.parameters.get("show_values").and_then(|v| v.as_bool()).unwrap_or(false);

        match action.as_str() {
            "list" => list_env(&env_file, show_values),
            "get" => {
                let key = match key {
                    Some(k) => k,
                    None => return err("Missing 'key' parameter for get action"),
                };
                get_env(&env_file, &key)
            }
            "set" => {
                let key = match key {
                    Some(k) => k,
                    None => return err("Missing 'key' parameter for set action"),
                };
                let value = match value {
                    Some(v) => v,
                    None => return err("Missing 'value' parameter for set action"),
                };
                set_env(&env_file, &key, &value)
            }
            "delete" => {
                let key = match key {
                    Some(k) => k,
                    None => return err("Missing 'key' parameter for delete action"),
                };
                delete_env(&env_file, &key)
            }
            _ => err(format!("Unknown action: {action}")),
        }
    }
}

fn parse_env_file(path: &str) -> Result<Vec<(String, String)>, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {path}: {e}"))?;

    let mut entries = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((key, val)) = trimmed.split_once('=') {
            let key = key.trim().to_string();
            let val = val.trim().trim_matches('"').trim_matches('\'').to_string();
            entries.push((key, val));
        }
    }
    Ok(entries)
}

fn write_env_file(path: &str, entries: &[(String, String)]) -> Result<(), String> {
    let content: String = entries
        .iter()
        .map(|(k, v)| {
            if v.contains(' ') || v.contains('#') || v.contains('=') {
                format!("{k}=\"{v}\"\n")
            } else {
                format!("{k}={v}\n")
            }
        })
        .collect();

    std::fs::write(path, content).map_err(|e| format!("Failed to write {path}: {e}"))
}

fn list_env(path: &str, show_values: bool) -> ToolOutput {
    match parse_env_file(path) {
        Ok(entries) => {
            let vars: Vec<Value> = entries
                .iter()
                .map(|(k, v)| {
                    if show_values {
                        json!({ "key": k, "value": v })
                    } else {
                        let masked = if v.len() > 4 {
                            format!("{}****", &v[..2])
                        } else {
                            "****".to_string()
                        };
                        json!({ "key": k, "value": masked })
                    }
                })
                .collect();
            let count = vars.len();
            ok(json!({ "variables": vars, "count": count, "env_file": path }))
        }
        Err(e) => err(e),
    }
}

fn get_env(path: &str, key: &str) -> ToolOutput {
    match parse_env_file(path) {
        Ok(entries) => {
            match entries.iter().find(|(k, _)| k == key) {
                Some((_, v)) => ok(json!({ "key": key, "value": v })),
                None => err(format!("Key '{key}' not found in {path}")),
            }
        }
        Err(e) => err(e),
    }
}

fn set_env(path: &str, key: &str, value: &str) -> ToolOutput {
    let mut entries = parse_env_file(path).unwrap_or_default();

    let mut found = false;
    for (k, v) in entries.iter_mut() {
        if k == key {
            *v = value.to_string();
            found = true;
            break;
        }
    }
    if !found {
        entries.push((key.to_string(), value.to_string()));
    }

    match write_env_file(path, &entries) {
        Ok(()) => ok(json!({
            "key": key,
            "action": if found { "updated" } else { "added" },
            "env_file": path
        })),
        Err(e) => err(e),
    }
}

fn delete_env(path: &str, key: &str) -> ToolOutput {
    match parse_env_file(path) {
        Ok(entries) => {
            let original_len = entries.len();
            let filtered: Vec<(String, String)> = entries
                .into_iter()
                .filter(|(k, _)| k != key)
                .collect();
            let deleted = original_len != filtered.len();

            if !deleted {
                return err(format!("Key '{key}' not found in {path}"));
            }

            match write_env_file(path, &filtered) {
                Ok(()) => ok(json!({ "key": key, "deleted": true, "env_file": path })),
                Err(e) => err(e),
            }
        }
        Err(e) => err(e),
    }
}

// ---------------------------------------------------------------------------
// DockerfileGenerateTool
// ---------------------------------------------------------------------------

/// Generate a Dockerfile for a project.
pub struct DockerfileGenerateTool;

#[async_trait]
impl Tool for DockerfileGenerateTool {
    fn name(&self) -> &str {
        "dockerfile_generate"
    }

    fn description(&self) -> &str {
        "Generate a Dockerfile for a project by detecting the language/framework. Optionally specify a base image and entrypoint."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "project_path": { "type": "string", "description": "Path to the project directory" },
                "base_image": { "type": "string", "description": "Base Docker image (optional, auto-detected)" },
                "entrypoint": { "type": "string", "description": "Container entrypoint command (optional, auto-detected)" },
                "output_path": { "type": "string", "description": "Where to write the Dockerfile (default: <project_path>/Dockerfile)" },
                "multi_stage": { "type": "boolean", "description": "Use multi-stage build (default: true)" }
            },
            "required": ["project_path"]
        })
    }

    fn category(&self) -> &str {
        "build_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let project_path = match input.parameters.get("project_path").and_then(|v| v.as_str()) {
            Some(p) => p.to_string(),
            None => return err("Missing required parameter: project_path"),
        };
        let base_image = input.parameters.get("base_image").and_then(|v| v.as_str()).map(|s| s.to_string());
        let entrypoint = input.parameters.get("entrypoint").and_then(|v| v.as_str()).map(|s| s.to_string());
        let multi_stage = input.parameters.get("multi_stage").and_then(|v| v.as_bool()).unwrap_or(true);
        let output_path = input
            .parameters
            .get("output_path")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("{project_path}/Dockerfile"));

        let p = Path::new(&project_path);
        if !p.exists() {
            return err(format!("Project path does not exist: {project_path}"));
        }

        // Detect project type
        let dockerfile = if p.join("Cargo.toml").exists() {
            generate_rust_dockerfile(&base_image, &entrypoint, multi_stage)
        } else if p.join("package.json").exists() {
            let is_next = p.join("next.config.js").exists() || p.join("next.config.mjs").exists() || p.join("next.config.ts").exists();
            if is_next {
                generate_nextjs_dockerfile(&base_image, &entrypoint, multi_stage)
            } else {
                generate_node_dockerfile(&base_image, &entrypoint, multi_stage)
            }
        } else if p.join("requirements.txt").exists() || p.join("pyproject.toml").exists() {
            generate_python_dockerfile(&base_image, &entrypoint)
        } else if p.join("go.mod").exists() {
            generate_go_dockerfile(&base_image, &entrypoint, multi_stage)
        } else {
            return err("Cannot detect project type. Provide 'base_image' and 'entrypoint' manually.");
        };

        if let Err(e) = std::fs::write(&output_path, &dockerfile) {
            return err(format!("Failed to write Dockerfile: {e}"));
        }

        ok(json!({
            "output_path": output_path,
            "content": dockerfile,
            "multi_stage": multi_stage
        }))
    }
}

fn generate_rust_dockerfile(base: &Option<String>, entry: &Option<String>, multi_stage: bool) -> String {
    let base_image = base.as_deref().unwrap_or("rust:1.77-slim");
    let entrypoint = entry.as_deref().unwrap_or("./target/release/app");

    if multi_stage {
        format!(
r#"# Build stage
FROM {base_image} AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {{}}' > src/main.rs && cargo build --release && rm -rf src
COPY . .
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/app .
EXPOSE 8080
CMD ["{entrypoint}"]
"#)
    } else {
        format!(
r#"FROM {base_image}
WORKDIR /app
COPY . .
RUN cargo build --release
EXPOSE 8080
CMD ["{entrypoint}"]
"#)
    }
}

fn generate_node_dockerfile(base: &Option<String>, entry: &Option<String>, multi_stage: bool) -> String {
    let base_image = base.as_deref().unwrap_or("node:20-alpine");
    let entrypoint = entry.as_deref().unwrap_or("node dist/index.js");

    if multi_stage {
        format!(
r#"# Build stage
FROM {base_image} AS builder
WORKDIR /app
COPY package*.json ./
RUN npm ci
COPY . .
RUN npm run build

# Runtime stage
FROM {base_image}
WORKDIR /app
COPY package*.json ./
RUN npm ci --production
COPY --from=builder /app/dist ./dist
EXPOSE 3000
CMD ["{entrypoint}"]
"#)
    } else {
        format!(
r#"FROM {base_image}
WORKDIR /app
COPY package*.json ./
RUN npm ci
COPY . .
RUN npm run build
EXPOSE 3000
CMD ["{entrypoint}"]
"#)
    }
}

fn generate_nextjs_dockerfile(base: &Option<String>, entry: &Option<String>, multi_stage: bool) -> String {
    let base_image = base.as_deref().unwrap_or("node:20-alpine");
    let _entrypoint = entry.as_deref().unwrap_or("node server.js");

    if multi_stage {
        format!(
r#"# Dependencies stage
FROM {base_image} AS deps
WORKDIR /app
COPY package*.json ./
RUN npm ci

# Build stage
FROM {base_image} AS builder
WORKDIR /app
COPY --from=deps /app/node_modules ./node_modules
COPY . .
RUN npm run build

# Runtime stage
FROM {base_image} AS runner
WORKDIR /app
ENV NODE_ENV=production
COPY --from=builder /app/public ./public
COPY --from=builder /app/.next/standalone ./
COPY --from=builder /app/.next/static ./.next/static
EXPOSE 3000
ENV PORT=3000
CMD ["node", "server.js"]
"#)
    } else {
        format!(
r#"FROM {base_image}
WORKDIR /app
COPY . .
RUN npm ci && npm run build
EXPOSE 3000
CMD ["npm", "start"]
"#)
    }
}

fn generate_python_dockerfile(base: &Option<String>, entry: &Option<String>) -> String {
    let base_image = base.as_deref().unwrap_or("python:3.12-slim");
    let entrypoint = entry.as_deref().unwrap_or("python main.py");

    format!(
r#"FROM {base_image}
WORKDIR /app
COPY requirements.txt ./
RUN pip install --no-cache-dir -r requirements.txt
COPY . .
EXPOSE 8000
CMD ["{entrypoint}"]
"#)
}

fn generate_go_dockerfile(base: &Option<String>, entry: &Option<String>, multi_stage: bool) -> String {
    let base_image = base.as_deref().unwrap_or("golang:1.22-alpine");
    let entrypoint = entry.as_deref().unwrap_or("./app");

    if multi_stage {
        format!(
r#"# Build stage
FROM {base_image} AS builder
WORKDIR /app
COPY go.mod go.sum ./
RUN go mod download
COPY . .
RUN CGO_ENABLED=0 go build -o app .

# Runtime stage
FROM alpine:3.19
RUN apk add --no-cache ca-certificates
WORKDIR /app
COPY --from=builder /app/app .
EXPOSE 8080
CMD ["{entrypoint}"]
"#)
    } else {
        format!(
r#"FROM {base_image}
WORKDIR /app
COPY . .
RUN go build -o app .
EXPOSE 8080
CMD ["{entrypoint}"]
"#)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docker_build_tool_metadata() {
        let tool = DockerBuildTool;
        assert_eq!(tool.name(), "docker_build");
        assert_eq!(tool.category(), "build_ops");
    }

    #[test]
    fn docker_run_tool_metadata() {
        let tool = DockerRunTool;
        assert_eq!(tool.name(), "docker_run");
        assert_eq!(tool.category(), "build_ops");
    }

    #[test]
    fn system_info_tool_metadata() {
        let tool = SystemInfoTool;
        assert_eq!(tool.name(), "system_info");
        assert_eq!(tool.category(), "data_ops");
    }

    #[test]
    fn env_manage_tool_metadata() {
        let tool = EnvManageTool;
        assert_eq!(tool.name(), "env_manage");
        assert_eq!(tool.category(), "data_ops");
    }

    #[test]
    fn dockerfile_generate_tool_metadata() {
        let tool = DockerfileGenerateTool;
        assert_eq!(tool.name(), "dockerfile_generate");
        assert_eq!(tool.category(), "build_ops");
    }

    #[tokio::test]
    async fn system_info_works() {
        let tool = SystemInfoTool;
        let out = tool
            .execute(ToolInput {
                parameters: json!({}),
            })
            .await;
        assert!(out.success);
        assert!(out.result["os"].as_str().is_some());
        assert!(out.result["cpu_count"].as_u64().unwrap() > 0);
    }

    #[test]
    fn generate_rust_dockerfile_multi_stage() {
        let df = generate_rust_dockerfile(&None, &None, true);
        assert!(df.contains("FROM rust:"));
        assert!(df.contains("AS builder"));
        assert!(df.contains("debian:bookworm-slim"));
    }

    #[test]
    fn generate_node_dockerfile_single_stage() {
        let df = generate_node_dockerfile(&None, &None, false);
        assert!(df.contains("FROM node:"));
        assert!(!df.contains("AS builder"));
    }
}
