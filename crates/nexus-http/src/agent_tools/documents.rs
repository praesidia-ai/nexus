//! Document tools — data format conversion and spreadsheet generation.
//!
//! These tools handle transforming between structured data formats
//! (JSON, YAML, TOML, CSV) and generating CSV files.

use async_trait::async_trait;
use serde_json::{json, Value};

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

// ---------------------------------------------------------------------------
// DataTransformTool
// ---------------------------------------------------------------------------

/// Convert between JSON, YAML, TOML, and CSV formats.
pub struct DataTransformTool;

#[async_trait]
impl Tool for DataTransformTool {
    fn name(&self) -> &str {
        "data_transform"
    }

    fn description(&self) -> &str {
        "Convert data between JSON, YAML, TOML, and CSV formats. Input a string in one format, get output in another."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "input": { "type": "string", "description": "Input data as a string" },
                "from_format": { "type": "string", "description": "Input format", "enum": ["json", "yaml", "toml", "csv"] },
                "to_format": { "type": "string", "description": "Output format", "enum": ["json", "yaml", "toml", "csv"] },
                "pretty": { "type": "boolean", "description": "Pretty-print output (default: true)" }
            },
            "required": ["input", "from_format", "to_format"]
        })
    }

    fn category(&self) -> &str {
        "data_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let data = match input.parameters.get("input").and_then(|v| v.as_str()) {
            Some(d) => d,
            None => return err("Missing required parameter: input"),
        };
        let from = match input.parameters.get("from_format").and_then(|v| v.as_str()) {
            Some(f) => f,
            None => return err("Missing required parameter: from_format"),
        };
        let to = match input.parameters.get("to_format").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => return err("Missing required parameter: to_format"),
        };
        let pretty = input
            .parameters
            .get("pretty")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        // Step 1: Parse input into a serde_json::Value
        let value: Value = match from {
            "json" => match serde_json::from_str(data) {
                Ok(v) => v,
                Err(e) => return err(format!("Invalid JSON input: {e}")),
            },
            "yaml" => {
                // Simple YAML-to-JSON conversion without serde_yaml dependency
                // We parse basic YAML key: value pairs
                match parse_simple_yaml(data) {
                    Ok(v) => v,
                    Err(e) => return err(format!("YAML parse error: {e}")),
                }
            }
            "toml" => {
                // Simple TOML-to-JSON conversion without toml dependency
                match parse_simple_toml(data) {
                    Ok(v) => v,
                    Err(e) => return err(format!("TOML parse error: {e}")),
                }
            }
            "csv" => {
                // Parse CSV into array of objects
                let lines: Vec<&str> = data.lines().collect();
                if lines.is_empty() {
                    json!([])
                } else {
                    let headers: Vec<&str> = lines[0].split(',').map(|s| s.trim()).collect();
                    let rows: Vec<Value> = lines[1..]
                        .iter()
                        .filter(|l| !l.trim().is_empty())
                        .map(|line| {
                            let fields: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
                            let mut obj = serde_json::Map::new();
                            for (i, header) in headers.iter().enumerate() {
                                let val = fields.get(i).copied().unwrap_or("");
                                obj.insert(header.to_string(), Value::String(val.to_string()));
                            }
                            Value::Object(obj)
                        })
                        .collect();
                    Value::Array(rows)
                }
            }
            _ => return err(format!("Unsupported input format: {from}")),
        };

        // Step 2: Serialize to target format
        let output = match to {
            "json" => {
                if pretty {
                    serde_json::to_string_pretty(&value).unwrap_or_default()
                } else {
                    serde_json::to_string(&value).unwrap_or_default()
                }
            }
            "yaml" => value_to_yaml(&value, 0),
            "toml" => value_to_toml(&value),
            "csv" => value_to_csv(&value),
            _ => return err(format!("Unsupported output format: {to}")),
        };

        ok(json!({
            "output": output,
            "from_format": from,
            "to_format": to,
            "length": output.len()
        }))
    }
}

/// Parse simple YAML (key: value pairs, nested objects via indentation).
fn parse_simple_yaml(input: &str) -> Result<Value, String> {
    let mut root = serde_json::Map::new();

    for line in input.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if let Some((key, val)) = trimmed.split_once(':') {
            let key = key.trim().trim_matches('"');
            let val = val.trim();

            if val.is_empty() {
                root.insert(key.to_string(), Value::Null);
            } else if val == "true" || val == "True" {
                root.insert(key.to_string(), Value::Bool(true));
            } else if val == "false" || val == "False" {
                root.insert(key.to_string(), Value::Bool(false));
            } else if val == "null" || val == "~" {
                root.insert(key.to_string(), Value::Null);
            } else if let Ok(n) = val.parse::<i64>() {
                root.insert(key.to_string(), json!(n));
            } else if let Ok(f) = val.parse::<f64>() {
                root.insert(key.to_string(), json!(f));
            } else {
                let val_str = val.trim_matches('"').trim_matches('\'');
                root.insert(key.to_string(), Value::String(val_str.to_string()));
            }
        }
    }

    Ok(Value::Object(root))
}

/// Parse simple TOML (key = value pairs, [sections]).
fn parse_simple_toml(input: &str) -> Result<Value, String> {
    let mut root = serde_json::Map::new();
    let mut current_section: Option<String> = None;

    for line in input.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Section header [section]
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let section = trimmed[1..trimmed.len() - 1].trim().to_string();
            current_section = Some(section.clone());
            if !root.contains_key(&section) {
                root.insert(section, Value::Object(serde_json::Map::new()));
            }
            continue;
        }

        if let Some((key, val)) = trimmed.split_once('=') {
            let key = key.trim().trim_matches('"');
            let val = val.trim();

            let parsed_val = if val.starts_with('"') && val.ends_with('"') {
                Value::String(val[1..val.len() - 1].to_string())
            } else if val == "true" {
                Value::Bool(true)
            } else if val == "false" {
                Value::Bool(false)
            } else if let Ok(n) = val.parse::<i64>() {
                json!(n)
            } else if let Ok(f) = val.parse::<f64>() {
                json!(f)
            } else {
                Value::String(val.to_string())
            };

            if let Some(ref section) = current_section {
                if let Some(Value::Object(map)) = root.get_mut(section) {
                    map.insert(key.to_string(), parsed_val);
                }
            } else {
                root.insert(key.to_string(), parsed_val);
            }
        }
    }

    Ok(Value::Object(root))
}

/// Convert a JSON value to YAML-like string.
fn value_to_yaml(value: &Value, indent: usize) -> String {
    let prefix = "  ".repeat(indent);
    match value {
        Value::Object(map) => {
            let mut out = String::new();
            for (key, val) in map {
                match val {
                    Value::Object(_) => {
                        out.push_str(&format!("{prefix}{key}:\n"));
                        out.push_str(&value_to_yaml(val, indent + 1));
                    }
                    Value::Array(arr) => {
                        out.push_str(&format!("{prefix}{key}:\n"));
                        for item in arr {
                            out.push_str(&format!("{prefix}  - "));
                            match item {
                                Value::Object(_) => {
                                    out.push('\n');
                                    out.push_str(&value_to_yaml(item, indent + 2));
                                }
                                _ => {
                                    out.push_str(&format_yaml_scalar(item));
                                    out.push('\n');
                                }
                            }
                        }
                    }
                    _ => {
                        out.push_str(&format!("{prefix}{key}: {}\n", format_yaml_scalar(val)));
                    }
                }
            }
            out
        }
        Value::Array(arr) => {
            let mut out = String::new();
            for item in arr {
                out.push_str(&format!("{prefix}- "));
                match item {
                    Value::Object(_) => {
                        out.push('\n');
                        out.push_str(&value_to_yaml(item, indent + 1));
                    }
                    _ => {
                        out.push_str(&format_yaml_scalar(item));
                        out.push('\n');
                    }
                }
            }
            out
        }
        _ => format!("{}\n", format_yaml_scalar(value)),
    }
}

fn format_yaml_scalar(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => {
            if s.contains(':') || s.contains('#') || s.contains('\n') {
                format!("\"{s}\"")
            } else {
                s.clone()
            }
        }
        _ => serde_json::to_string(value).unwrap_or_default(),
    }
}

/// Convert a JSON value to TOML-like string.
fn value_to_toml(value: &Value) -> String {
    let mut out = String::new();
    if let Value::Object(map) = value {
        // First, output non-object/non-array values
        for (key, val) in map {
            match val {
                Value::Object(_) | Value::Array(_) => {}
                _ => {
                    out.push_str(&format!("{key} = {}\n", format_toml_value(val)));
                }
            }
        }
        // Then output sections
        for (key, val) in map {
            if let Value::Object(inner) = val {
                out.push_str(&format!("\n[{key}]\n"));
                for (k, v) in inner {
                    out.push_str(&format!("{k} = {}\n", format_toml_value(v)));
                }
            }
        }
    }
    out
}

fn format_toml_value(value: &Value) -> String {
    match value {
        Value::Null => "\"\"".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => format!("\"{s}\""),
        Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(format_toml_value).collect();
            format!("[{}]", items.join(", "))
        }
        Value::Object(_) => "{}".to_string(),
    }
}

/// Convert a JSON value (array of objects) to CSV string.
fn value_to_csv(value: &Value) -> String {
    match value {
        Value::Array(arr) if !arr.is_empty() => {
            // Collect headers from first object
            let headers: Vec<String> = if let Some(Value::Object(first)) = arr.first() {
                first.keys().cloned().collect()
            } else {
                return String::new();
            };

            let mut out = headers.join(",");
            out.push('\n');

            for item in arr {
                if let Value::Object(obj) = item {
                    let row: Vec<String> = headers
                        .iter()
                        .map(|h| {
                            let val = obj.get(h).unwrap_or(&Value::Null);
                            match val {
                                Value::String(s) => {
                                    if s.contains(',') || s.contains('"') || s.contains('\n') {
                                        format!("\"{}\"", s.replace('"', "\"\""))
                                    } else {
                                        s.clone()
                                    }
                                }
                                Value::Null => String::new(),
                                _ => val.to_string(),
                            }
                        })
                        .collect();
                    out.push_str(&row.join(","));
                    out.push('\n');
                }
            }
            out
        }
        _ => String::new(),
    }
}

// ---------------------------------------------------------------------------
// SpreadsheetTool
// ---------------------------------------------------------------------------

/// Generate a CSV file from structured data.
pub struct SpreadsheetTool;

#[async_trait]
impl Tool for SpreadsheetTool {
    fn name(&self) -> &str {
        "spreadsheet"
    }

    fn description(&self) -> &str {
        "Generate a CSV file from structured data. Provide headers and rows, get a CSV file written to disk."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "headers": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Column headers"
                },
                "rows": {
                    "type": "array",
                    "items": {
                        "type": "array",
                        "items": { "type": "string" }
                    },
                    "description": "Array of rows, each an array of cell values"
                },
                "output_path": { "type": "string", "description": "Path to write the CSV file" },
                "delimiter": { "type": "string", "description": "Delimiter character (default: ',')" }
            },
            "required": ["headers", "rows", "output_path"]
        })
    }

    fn category(&self) -> &str {
        "data_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let headers: Vec<String> = match input.parameters.get("headers").and_then(|v| v.as_array()) {
            Some(arr) => arr.iter().filter_map(|v| v.as_str().map(String::from)).collect(),
            None => return err("Missing required parameter: headers"),
        };
        let rows: Vec<Vec<String>> = match input.parameters.get("rows").and_then(|v| v.as_array()) {
            Some(arr) => arr
                .iter()
                .filter_map(|row| {
                    row.as_array().map(|cells| {
                        cells
                            .iter()
                            .map(|c| match c {
                                Value::String(s) => s.clone(),
                                Value::Null => String::new(),
                                other => other.to_string(),
                            })
                            .collect()
                    })
                })
                .collect(),
            None => return err("Missing required parameter: rows"),
        };
        let output_path = match input.parameters.get("output_path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return err("Missing required parameter: output_path"),
        };
        let delimiter = input
            .parameters
            .get("delimiter")
            .and_then(|v| v.as_str())
            .and_then(|s| s.chars().next())
            .unwrap_or(',');

        let mut csv = String::new();

        // Header row
        let escaped_headers: Vec<String> = headers.iter().map(|h| escape_csv_field(h, delimiter)).collect();
        csv.push_str(&escaped_headers.join(&delimiter.to_string()));
        csv.push('\n');

        // Data rows
        for row in &rows {
            let escaped_cells: Vec<String> = row.iter().map(|c| escape_csv_field(c, delimiter)).collect();
            csv.push_str(&escaped_cells.join(&delimiter.to_string()));
            csv.push('\n');
        }

        // Write to file
        if let Err(e) = std::fs::write(output_path, &csv) {
            return err(format!("Failed to write CSV to {output_path}: {e}"));
        }

        ok(json!({
            "output_path": output_path,
            "headers": headers,
            "row_count": rows.len(),
            "bytes_written": csv.len()
        }))
    }
}

/// Escape a CSV field value, quoting if necessary.
fn escape_csv_field(field: &str, delimiter: char) -> String {
    if field.contains(delimiter) || field.contains('"') || field.contains('\n') {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_transform_tool_metadata() {
        let tool = DataTransformTool;
        assert_eq!(tool.name(), "data_transform");
        assert_eq!(tool.category(), "data_ops");
    }

    #[test]
    fn spreadsheet_tool_metadata() {
        let tool = SpreadsheetTool;
        assert_eq!(tool.name(), "spreadsheet");
        assert_eq!(tool.category(), "data_ops");
    }

    #[tokio::test]
    async fn data_transform_json_to_csv() {
        let tool = DataTransformTool;
        let out = tool
            .execute(ToolInput {
                parameters: json!({
                    "input": "[{\"name\":\"Alice\",\"age\":\"30\"},{\"name\":\"Bob\",\"age\":\"25\"}]",
                    "from_format": "json",
                    "to_format": "csv"
                }),
            })
            .await;
        assert!(out.success);
        let output = out.result["output"].as_str().unwrap();
        assert!(output.contains("name"));
        assert!(output.contains("Alice"));
    }

    #[tokio::test]
    async fn data_transform_csv_to_json() {
        let tool = DataTransformTool;
        let out = tool
            .execute(ToolInput {
                parameters: json!({
                    "input": "name,age\nAlice,30\nBob,25",
                    "from_format": "csv",
                    "to_format": "json"
                }),
            })
            .await;
        assert!(out.success);
        let output = out.result["output"].as_str().unwrap();
        assert!(output.contains("Alice"));
    }

    #[tokio::test]
    async fn data_transform_json_to_yaml() {
        let tool = DataTransformTool;
        let out = tool
            .execute(ToolInput {
                parameters: json!({
                    "input": "{\"name\":\"test\",\"version\":1}",
                    "from_format": "json",
                    "to_format": "yaml"
                }),
            })
            .await;
        assert!(out.success);
        let output = out.result["output"].as_str().unwrap();
        assert!(output.contains("name: test"));
    }

    #[test]
    fn escape_csv_field_normal() {
        assert_eq!(escape_csv_field("hello", ','), "hello");
        assert_eq!(escape_csv_field("hello,world", ','), "\"hello,world\"");
        assert_eq!(escape_csv_field("say \"hi\"", ','), "\"say \"\"hi\"\"\"");
    }

    #[test]
    fn parse_simple_yaml_basic() {
        let yaml = "name: test\nversion: 1\nenabled: true";
        let result = parse_simple_yaml(yaml).unwrap();
        assert_eq!(result["name"], "test");
        assert_eq!(result["version"], 1);
        assert_eq!(result["enabled"], true);
    }

    #[test]
    fn parse_simple_toml_basic() {
        let toml_str = "name = \"test\"\nversion = 1\n\n[package]\nauthor = \"alice\"";
        let result = parse_simple_toml(toml_str).unwrap();
        assert_eq!(result["name"], "test");
        assert_eq!(result["package"]["author"], "alice");
    }
}
