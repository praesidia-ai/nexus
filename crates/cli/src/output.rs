//! Output formatting utilities for the Nexus CLI.
//!
//! Supports human-readable, JSON, and quiet output modes.

use std::fmt;
use std::str::FromStr;

/// Output format for CLI responses.
#[derive(Debug, Clone, Default)]
pub enum OutputFormat {
    /// Human-readable colored output (default).
    #[default]
    Human,
    /// Raw JSON output for scripting.
    Json,
    /// Minimal output — only errors and essential info.
    Quiet,
}

impl fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OutputFormat::Human => write!(f, "human"),
            OutputFormat::Json => write!(f, "json"),
            OutputFormat::Quiet => write!(f, "quiet"),
        }
    }
}

impl FromStr for OutputFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "human" => Ok(OutputFormat::Human),
            "json" => Ok(OutputFormat::Json),
            "quiet" => Ok(OutputFormat::Quiet),
            other => Err(format!(
                "Unknown output format '{}'. Expected: human, json, quiet",
                other
            )),
        }
    }
}

/// Print a JSON value with pretty formatting.
pub fn print_json(value: &serde_json::Value) {
    match serde_json::to_string_pretty(value) {
        Ok(s) => println!("{}", s),
        Err(e) => print_error(&format!("Failed to serialize JSON: {}", e)),
    }
}

/// Print a table with headers and rows.
pub fn print_table(headers: &[&str], rows: &[Vec<String>]) {
    if headers.is_empty() {
        return;
    }

    // Calculate column widths
    let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < widths.len() && cell.len() > widths[i] {
                widths[i] = cell.len();
            }
        }
    }

    // Print header
    let header_line: Vec<String> = headers
        .iter()
        .enumerate()
        .map(|(i, h)| format!("{:<width$}", h, width = widths[i]))
        .collect();
    println!("  {}", header_line.join("  "));

    // Print separator
    let sep_line: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
    println!("  {}", sep_line.join("  "));

    // Print rows
    for row in rows {
        let row_line: Vec<String> = row
            .iter()
            .enumerate()
            .map(|(i, cell)| {
                let width = widths.get(i).copied().unwrap_or(cell.len());
                format!("{:<width$}", cell, width = width)
            })
            .collect();
        println!("  {}", row_line.join("  "));
    }
}

/// Print a success message.
pub fn print_success(msg: &str) {
    println!("[OK] {}", msg);
}

/// Print an error message to stderr.
pub fn print_error(msg: &str) {
    eprintln!("[ERROR] {}", msg);
}

/// Print a warning message.
pub fn print_warning(msg: &str) {
    eprintln!("[WARN] {}", msg);
}

/// Print a progress indicator.
pub fn print_progress(msg: &str, percent: u32) {
    let clamped = percent.min(100);
    let filled = (clamped as usize) / 5;
    let empty = 20 - filled;
    let bar = format!(
        "[{}{}] {}%",
        "#".repeat(filled),
        ".".repeat(empty),
        clamped
    );
    println!("{} {}", bar, msg);
}

/// Print a section header.
pub fn print_header(title: &str) {
    println!();
    println!("=== {} ===", title);
    println!();
}

/// Print a key-value pair with alignment.
pub fn print_kv(key: &str, value: &str) {
    println!("  {:<16} {}", format!("{}:", key), value);
}
