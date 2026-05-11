//! Data tools — JSON parsing, CSV parsing, and math evaluation.
//!
//! These tools help agents transform and compute data inline without
//! shelling out to external programs.

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
// JsonParseTool
// ---------------------------------------------------------------------------

/// Parse a JSON string and optionally extract a field via dot-path.
pub struct JsonParseTool;

#[async_trait]
impl Tool for JsonParseTool {
    fn name(&self) -> &str {
        "json_parse"
    }

    fn description(&self) -> &str {
        "Parse a JSON string. Optionally extract a value using a dot-separated path (e.g. 'data.items.0.name')."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "text": { "type": "string", "description": "JSON string to parse" },
                "path": { "type": "string", "description": "Dot-separated path to extract (optional)" }
            },
            "required": ["text"]
        })
    }

    fn category(&self) -> &str {
        "data_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let text = match input.parameters.get("text").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => return err("Missing required parameter: text"),
        };

        let parsed: Value = match serde_json::from_str(text) {
            Ok(v) => v,
            Err(e) => return err(format!("Invalid JSON: {}", e)),
        };

        let path = input.parameters.get("path").and_then(|v| v.as_str());

        match path {
            None => ok(json!({ "value": parsed })),
            Some(dot_path) => {
                let extracted = extract_path(&parsed, dot_path);
                match extracted {
                    Some(v) => ok(json!({ "value": v })),
                    None => err(format!("Path '{}' not found in JSON", dot_path)),
                }
            }
        }
    }
}

/// Walk a JSON value using a dot-separated path like `data.items.0.name`.
fn extract_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = value;
    for segment in path.split('.') {
        if segment.is_empty() {
            continue;
        }
        // Try as object key first
        if let Some(v) = current.get(segment) {
            current = v;
            continue;
        }
        // Try as array index
        if let Ok(idx) = segment.parse::<usize>() {
            if let Some(v) = current.get(idx) {
                current = v;
                continue;
            }
        }
        return None;
    }
    Some(current)
}

// ---------------------------------------------------------------------------
// CsvParseTool
// ---------------------------------------------------------------------------

/// Parse CSV text into a JSON array of objects.
///
/// The first row is treated as headers by default.
pub struct CsvParseTool;

#[async_trait]
impl Tool for CsvParseTool {
    fn name(&self) -> &str {
        "csv_parse"
    }

    fn description(&self) -> &str {
        "Parse CSV text into a JSON array of objects. First row is used as column headers."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "text": { "type": "string", "description": "CSV text to parse" },
                "delimiter": { "type": "string", "description": "Delimiter character (default: ',')" },
                "has_headers": { "type": "boolean", "description": "First row is headers (default: true)" }
            },
            "required": ["text"]
        })
    }

    fn category(&self) -> &str {
        "data_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let text = match input.parameters.get("text").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => return err("Missing required parameter: text"),
        };

        let delimiter = input
            .parameters
            .get("delimiter")
            .and_then(|v| v.as_str())
            .and_then(|s| s.chars().next())
            .unwrap_or(',');

        let has_headers = input
            .parameters
            .get("has_headers")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let lines: Vec<&str> = text.lines().collect();
        if lines.is_empty() {
            return ok(json!({ "rows": [], "count": 0 }));
        }

        let split_row = |line: &str| -> Vec<String> {
            let mut fields = Vec::new();
            let mut current = String::new();
            let mut in_quotes = false;

            for ch in line.chars() {
                if ch == '"' {
                    in_quotes = !in_quotes;
                } else if ch == delimiter && !in_quotes {
                    fields.push(current.trim().to_string());
                    current = String::new();
                } else {
                    current.push(ch);
                }
            }
            fields.push(current.trim().to_string());
            fields
        };

        if has_headers && lines.len() >= 2 {
            let headers = split_row(lines[0]);
            let rows: Vec<Value> = lines[1..]
                .iter()
                .filter(|l| !l.trim().is_empty())
                .map(|line| {
                    let fields = split_row(line);
                    let mut obj = serde_json::Map::new();
                    for (i, header) in headers.iter().enumerate() {
                        let value = fields.get(i).cloned().unwrap_or_default();
                        obj.insert(header.clone(), Value::String(value));
                    }
                    Value::Object(obj)
                })
                .collect();

            let count = rows.len();
            ok(json!({ "rows": rows, "headers": headers, "count": count }))
        } else {
            // No headers — return arrays
            let rows: Vec<Value> = lines
                .iter()
                .filter(|l| !l.trim().is_empty())
                .map(|line| {
                    let fields = split_row(line);
                    Value::Array(fields.into_iter().map(Value::String).collect())
                })
                .collect();

            let count = rows.len();
            ok(json!({ "rows": rows, "count": count }))
        }
    }
}

// ---------------------------------------------------------------------------
// CalculateTool
// ---------------------------------------------------------------------------

/// Evaluate simple mathematical expressions.
///
/// Supports: `+`, `-`, `*`, `/`, `%`, parentheses, and common functions
/// (`sqrt`, `pow`, `abs`, `floor`, `ceil`, `round`, `min`, `max`).
pub struct CalculateTool;

#[async_trait]
impl Tool for CalculateTool {
    fn name(&self) -> &str {
        "calculate"
    }

    fn description(&self) -> &str {
        "Evaluate a mathematical expression. Supports +, -, *, /, %, parentheses, sqrt, pow, abs, floor, ceil, round."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "expression": { "type": "string", "description": "Mathematical expression to evaluate" }
            },
            "required": ["expression"]
        })
    }

    fn category(&self) -> &str {
        "data_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let expr = match input.parameters.get("expression").and_then(|v| v.as_str()) {
            Some(e) => e,
            None => return err("Missing required parameter: expression"),
        };

        match eval_expr(expr) {
            Ok(result) => ok(json!({
                "expression": expr,
                "result": result
            })),
            Err(e) => err(format!("Evaluation error: {}", e)),
        }
    }
}

// ---------------------------------------------------------------------------
// Simple expression evaluator (recursive descent parser)
// ---------------------------------------------------------------------------

/// Evaluate a mathematical expression string.
fn eval_expr(input: &str) -> Result<f64, String> {
    let tokens = tokenize(input)?;
    let mut pos = 0;
    let result = parse_addition(&tokens, &mut pos)?;
    if pos < tokens.len() {
        return Err(format!("Unexpected token at position {}", pos));
    }
    Ok(result)
}

#[derive(Debug, Clone)]
enum Token {
    Number(f64),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    LParen,
    RParen,
    Comma,
    Func(String),
}

fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        match chars[i] {
            ' ' | '\t' | '\n' => i += 1,
            '+' => {
                tokens.push(Token::Plus);
                i += 1;
            }
            '-' => {
                // Negative number: if at start or after operator/lparen
                let is_unary = tokens.is_empty()
                    || matches!(
                        tokens.last(),
                        Some(Token::Plus)
                            | Some(Token::Minus)
                            | Some(Token::Star)
                            | Some(Token::Slash)
                            | Some(Token::Percent)
                            | Some(Token::LParen)
                            | Some(Token::Comma)
                    );
                if is_unary && i + 1 < chars.len() && (chars[i + 1].is_ascii_digit() || chars[i + 1] == '.') {
                    let start = i;
                    i += 1;
                    while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                        i += 1;
                    }
                    let num_str: String = chars[start..i].iter().collect();
                    let num = num_str
                        .parse::<f64>()
                        .map_err(|_| format!("Invalid number: {}", num_str))?;
                    tokens.push(Token::Number(num));
                } else {
                    tokens.push(Token::Minus);
                    i += 1;
                }
            }
            '*' => {
                tokens.push(Token::Star);
                i += 1;
            }
            '/' => {
                tokens.push(Token::Slash);
                i += 1;
            }
            '%' => {
                tokens.push(Token::Percent);
                i += 1;
            }
            '(' => {
                tokens.push(Token::LParen);
                i += 1;
            }
            ')' => {
                tokens.push(Token::RParen);
                i += 1;
            }
            ',' => {
                tokens.push(Token::Comma);
                i += 1;
            }
            c if c.is_ascii_digit() || c == '.' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                let num_str: String = chars[start..i].iter().collect();
                let num = num_str
                    .parse::<f64>()
                    .map_err(|_| format!("Invalid number: {}", num_str))?;
                tokens.push(Token::Number(num));
            }
            c if c.is_ascii_alphabetic() => {
                let start = i;
                while i < chars.len() && chars[i].is_ascii_alphanumeric() {
                    i += 1;
                }
                let name: String = chars[start..i].iter().collect();

                // Check for known constants
                match name.to_lowercase().as_str() {
                    "pi" => tokens.push(Token::Number(std::f64::consts::PI)),
                    "e" => tokens.push(Token::Number(std::f64::consts::E)),
                    _ => tokens.push(Token::Func(name.to_lowercase())),
                }
            }
            c => return Err(format!("Unexpected character: '{}'", c)),
        }
    }

    Ok(tokens)
}

fn parse_addition(tokens: &[Token], pos: &mut usize) -> Result<f64, String> {
    let mut left = parse_multiplication(tokens, pos)?;
    while *pos < tokens.len() {
        match tokens[*pos] {
            Token::Plus => {
                *pos += 1;
                left += parse_multiplication(tokens, pos)?;
            }
            Token::Minus => {
                *pos += 1;
                left -= parse_multiplication(tokens, pos)?;
            }
            _ => break,
        }
    }
    Ok(left)
}

fn parse_multiplication(tokens: &[Token], pos: &mut usize) -> Result<f64, String> {
    let mut left = parse_primary(tokens, pos)?;
    while *pos < tokens.len() {
        match tokens[*pos] {
            Token::Star => {
                *pos += 1;
                left *= parse_primary(tokens, pos)?;
            }
            Token::Slash => {
                *pos += 1;
                let right = parse_primary(tokens, pos)?;
                if right == 0.0 {
                    return Err("Division by zero".to_string());
                }
                left /= right;
            }
            Token::Percent => {
                *pos += 1;
                let right = parse_primary(tokens, pos)?;
                if right == 0.0 {
                    return Err("Modulo by zero".to_string());
                }
                left %= right;
            }
            _ => break,
        }
    }
    Ok(left)
}

fn parse_primary(tokens: &[Token], pos: &mut usize) -> Result<f64, String> {
    if *pos >= tokens.len() {
        return Err("Unexpected end of expression".to_string());
    }

    match &tokens[*pos] {
        Token::Number(n) => {
            let val = *n;
            *pos += 1;
            Ok(val)
        }
        Token::LParen => {
            *pos += 1;
            let val = parse_addition(tokens, pos)?;
            if *pos >= tokens.len() {
                return Err("Missing closing parenthesis".to_string());
            }
            match tokens[*pos] {
                Token::RParen => {
                    *pos += 1;
                    Ok(val)
                }
                _ => Err("Expected closing parenthesis".to_string()),
            }
        }
        Token::Func(name) => {
            let func_name = name.clone();
            *pos += 1;
            // Expect opening paren
            if *pos >= tokens.len() || !matches!(tokens[*pos], Token::LParen) {
                return Err(format!("Expected '(' after function '{}'", func_name));
            }
            *pos += 1; // skip lparen

            // Parse comma-separated arguments
            let mut args = Vec::new();
            if *pos < tokens.len() && !matches!(tokens[*pos], Token::RParen) {
                args.push(parse_addition(tokens, pos)?);
                while *pos < tokens.len() && matches!(tokens[*pos], Token::Comma) {
                    *pos += 1;
                    args.push(parse_addition(tokens, pos)?);
                }
            }

            if *pos >= tokens.len() || !matches!(tokens[*pos], Token::RParen) {
                return Err(format!("Missing closing ')' for function '{}'", func_name));
            }
            *pos += 1; // skip rparen

            apply_function(&func_name, &args)
        }
        _ => Err(format!("Unexpected token at position {}", *pos)),
    }
}

fn apply_function(name: &str, args: &[f64]) -> Result<f64, String> {
    match name {
        "sqrt" => {
            ensure_args(name, args, 1)?;
            Ok(args[0].sqrt())
        }
        "abs" => {
            ensure_args(name, args, 1)?;
            Ok(args[0].abs())
        }
        "floor" => {
            ensure_args(name, args, 1)?;
            Ok(args[0].floor())
        }
        "ceil" => {
            ensure_args(name, args, 1)?;
            Ok(args[0].ceil())
        }
        "round" => {
            ensure_args(name, args, 1)?;
            Ok(args[0].round())
        }
        "sin" => {
            ensure_args(name, args, 1)?;
            Ok(args[0].sin())
        }
        "cos" => {
            ensure_args(name, args, 1)?;
            Ok(args[0].cos())
        }
        "tan" => {
            ensure_args(name, args, 1)?;
            Ok(args[0].tan())
        }
        "ln" | "log" => {
            ensure_args(name, args, 1)?;
            Ok(args[0].ln())
        }
        "log10" => {
            ensure_args(name, args, 1)?;
            Ok(args[0].log10())
        }
        "pow" => {
            ensure_args(name, args, 2)?;
            Ok(args[0].powf(args[1]))
        }
        "min" => {
            ensure_args(name, args, 2)?;
            Ok(args[0].min(args[1]))
        }
        "max" => {
            ensure_args(name, args, 2)?;
            Ok(args[0].max(args[1]))
        }
        _ => Err(format!("Unknown function: {}", name)),
    }
}

fn ensure_args(name: &str, args: &[f64], expected: usize) -> Result<(), String> {
    if args.len() != expected {
        Err(format!(
            "Function '{}' expects {} argument(s), got {}",
            name,
            expected,
            args.len()
        ))
    } else {
        Ok(())
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
    async fn json_parse_valid() {
        let tool = JsonParseTool;
        let out = tool
            .execute(input(json!({ "text": "{\"a\": 1, \"b\": [2,3]}" })))
            .await;

        assert!(out.success);
        assert_eq!(out.result["value"]["a"], 1);
    }

    #[tokio::test]
    async fn json_parse_with_path() {
        let tool = JsonParseTool;
        let out = tool
            .execute(input(json!({
                "text": "{\"data\": {\"items\": [{\"name\": \"first\"}]}}",
                "path": "data.items.0.name"
            })))
            .await;

        assert!(out.success);
        assert_eq!(out.result["value"], "first");
    }

    #[tokio::test]
    async fn json_parse_invalid() {
        let tool = JsonParseTool;
        let out = tool
            .execute(input(json!({ "text": "not json" })))
            .await;
        assert!(!out.success);
    }

    #[tokio::test]
    async fn csv_parse_with_headers() {
        let tool = CsvParseTool;
        let out = tool
            .execute(input(json!({
                "text": "name,age\nAlice,30\nBob,25"
            })))
            .await;

        assert!(out.success);
        assert_eq!(out.result["count"], 2);
        let rows = out.result["rows"].as_array().unwrap();
        assert_eq!(rows[0]["name"], "Alice");
        assert_eq!(rows[1]["age"], "25");
    }

    #[tokio::test]
    async fn csv_parse_custom_delimiter() {
        let tool = CsvParseTool;
        let out = tool
            .execute(input(json!({
                "text": "a;b;c\n1;2;3",
                "delimiter": ";"
            })))
            .await;

        assert!(out.success);
        assert_eq!(out.result["count"], 1);
    }

    #[test]
    fn eval_basic_arithmetic() {
        assert!((eval_expr("2 + 3").unwrap() - 5.0).abs() < f64::EPSILON);
        assert!((eval_expr("10 - 4").unwrap() - 6.0).abs() < f64::EPSILON);
        assert!((eval_expr("3 * 7").unwrap() - 21.0).abs() < f64::EPSILON);
        assert!((eval_expr("15 / 3").unwrap() - 5.0).abs() < f64::EPSILON);
        assert!((eval_expr("10 % 3").unwrap() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn eval_order_of_operations() {
        assert!((eval_expr("2 + 3 * 4").unwrap() - 14.0).abs() < f64::EPSILON);
        assert!((eval_expr("(2 + 3) * 4").unwrap() - 20.0).abs() < f64::EPSILON);
    }

    #[test]
    fn eval_functions() {
        assert!((eval_expr("sqrt(16)").unwrap() - 4.0).abs() < f64::EPSILON);
        assert!((eval_expr("pow(2, 10)").unwrap() - 1024.0).abs() < f64::EPSILON);
        assert!((eval_expr("abs(-5)").unwrap() - 5.0).abs() < f64::EPSILON);
        assert!((eval_expr("max(3, 7)").unwrap() - 7.0).abs() < f64::EPSILON);
        assert!((eval_expr("min(3, 7)").unwrap() - 3.0).abs() < f64::EPSILON);
        assert!((eval_expr("floor(3.7)").unwrap() - 3.0).abs() < f64::EPSILON);
        assert!((eval_expr("ceil(3.2)").unwrap() - 4.0).abs() < f64::EPSILON);
        assert!((eval_expr("round(3.5)").unwrap() - 4.0).abs() < f64::EPSILON);
    }

    #[test]
    fn eval_constants() {
        assert!((eval_expr("pi").unwrap() - std::f64::consts::PI).abs() < f64::EPSILON);
        assert!((eval_expr("e").unwrap() - std::f64::consts::E).abs() < f64::EPSILON);
    }

    #[test]
    fn eval_negative_numbers() {
        assert!((eval_expr("-5 + 3").unwrap() - (-2.0)).abs() < f64::EPSILON);
    }

    #[test]
    fn eval_division_by_zero() {
        assert!(eval_expr("1 / 0").is_err());
    }

    #[tokio::test]
    async fn calculate_tool_works() {
        let tool = CalculateTool;
        let out = tool
            .execute(input(json!({ "expression": "2 + 2" })))
            .await;

        assert!(out.success);
        assert_eq!(out.result["result"], 4.0);
    }
}
