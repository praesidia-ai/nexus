//! LLM-only code generation with continuation and multi-pass quality.
//!
//! Every generated app must come from the LLM. If the LLM fails or returns
//! output in the wrong format, we retry. We never silently fall back to static
//! templates — that would give the user a generic dummy app that has nothing to
//! do with what they asked to build.
//!
//! Generation strategy (inspired by how Bolt.new and Lovable achieve quality):
//!
//!   1. **Initial generation** — full prompt, expect === FILE: === blocks
//!   2. **Continuation** — if the response appears truncated (no END FILE on
//!      last block, or output ends mid-line), send a CONTINUE_PROMPT so the
//!      LLM resumes exactly where it left off. Up to 3 continuations.
//!   3. **Reformat retry** — if parse still returns 0 files, feed the LLM its
//!      own malformed output for self-correction.
//!   4. **Strict retry** — minimal prompt, maximum format enforcement.
//!
//! LLM transport errors (auth, network, rate-limit) are propagated immediately
//! because `call_llm_simple_for_project` already does 3 transport-level retries
//! with backoff internally.

use std::sync::Arc;

use tracing::info;

use crate::state::AppState;

const CONTINUE_PROMPT: &str =
    "Continue your prior response. IMPORTANT: Immediately begin from where you left off \
     without any interruptions. Do not repeat any content. Do not add any preamble or \
     explanation. Resume outputting file content exactly from the point you stopped.";

const MAX_CONTINUATIONS: usize = 3;

/// Detect whether the LLM response was truncated mid-generation.
///
/// Heuristics:
/// - The response has at least one `=== FILE:` but the last file block has
///   no matching `=== END FILE ===`.
/// - The response ends abruptly (no trailing newline after significant content).
fn looks_truncated(response: &str) -> bool {
    let trimmed = response.trim();
    if trimmed.is_empty() {
        return false;
    }

    let has_file_start = trimmed.contains("=== FILE:")
        || trimmed.contains("===FILE:")
        || trimmed.contains("--- FILE:");

    if !has_file_start {
        return false;
    }

    let last_file_start = trimmed.rfind("=== FILE:")
        .or_else(|| trimmed.rfind("===FILE:"))
        .or_else(|| trimmed.rfind("--- FILE:"));

    let last_file_end = trimmed.rfind("=== END FILE ===")
        .or_else(|| trimmed.rfind("===END FILE==="))
        .or_else(|| trimmed.rfind("--- END FILE ---"));

    match (last_file_start, last_file_end) {
        (Some(start), Some(end)) => start > end,
        (Some(_), None) => true,
        _ => false,
    }
}

/// Merge a continuation response onto the accumulated response.
///
/// The LLM should resume exactly where it stopped, so we simply concatenate.
/// But if the continuation starts with a file marker, we add a newline separator.
fn merge_continuation(accumulated: &mut String, continuation: &str) {
    let cont_trimmed = continuation.trim_start();
    if cont_trimmed.starts_with("=== FILE:")
        || cont_trimmed.starts_with("===FILE:")
        || cont_trimmed.starts_with("--- FILE:")
    {
        if !accumulated.ends_with('\n') {
            accumulated.push('\n');
        }
        accumulated.push_str("=== END FILE ===\n");
    }
    accumulated.push_str(continuation);
}

/// Validate generated files for obvious quality problems.
/// Returns a cleaned file list with empty/stub files removed.
fn validate_and_clean(files: Vec<(String, String)>) -> Vec<(String, String)> {
    files
        .into_iter()
        .filter(|(path, content)| {
            if content.trim().is_empty() {
                tracing::warn!(path = %path, "Removing empty generated file");
                return false;
            }
            if content.len() < 10 {
                tracing::warn!(path = %path, len = content.len(), "Removing suspiciously small file");
                return false;
            }
            true
        })
        .map(|(path, content)| {
            let cleaned = content
                .trim_start_matches("```typescript")
                .trim_start_matches("```tsx")
                .trim_start_matches("```python")
                .trim_start_matches("```go")
                .trim_start_matches("```rust")
                .trim_start_matches("```json")
                .trim_start_matches("```css")
                .trim_start_matches("```html")
                .trim_start_matches("```javascript")
                .trim_start_matches("```js")
                .trim_start_matches("```yaml")
                .trim_start_matches("```toml")
                .trim_start_matches("```sql")
                .trim_start_matches("```dart")
                .trim_start_matches("```ruby")
                .trim_start_matches("```php")
                .trim_start_matches("```java")
                .trim_start_matches("```kotlin")
                .trim_start_matches("```swift")
                .trim_start_matches("```")
                .trim_end_matches("```")
                .to_string();
            (path, cleaned)
        })
        .collect()
}

/// Call the LLM and parse the generated files.
///
/// Returns `Ok(files)` where `files` is a non-empty vec of `(path, content)` pairs.
/// Returns `Err(message)` if the LLM fails or refuses to output well-formed files
/// after all reformat retries.
pub async fn generate_app_files(
    app: &Arc<AppState>,
    prompt: &str,
    project_id: Option<&str>,
) -> Result<Vec<(String, String)>, String> {
    use nexus_store::parse_file_blocks;

    // ── Attempt 1: Initial generation + auto-continuation ────────────────
    let mut response = super::chat::call_llm_simple_for_project(app, prompt, project_id)
        .await
        .map_err(|e| format!("LLM call failed: {e}"))?;

    // Auto-continue if the response was truncated (LLM hit output token limit)
    for cont_idx in 0..MAX_CONTINUATIONS {
        if !looks_truncated(&response) {
            break;
        }
        info!(
            project = project_id.unwrap_or("-"),
            continuation = cont_idx + 1,
            response_len = response.len(),
            "Response appears truncated — sending continuation"
        );
        let continuation = super::chat::call_llm_simple_for_project(app, CONTINUE_PROMPT, project_id)
            .await
            .map_err(|e| format!("Continuation {} failed: {e}", cont_idx + 1))?;
        merge_continuation(&mut response, &continuation);
    }

    let files = validate_and_clean(parse_file_blocks(&response));
    if !files.is_empty() {
        info!(
            project = project_id.unwrap_or("-"),
            file_count = files.len(),
            "LLM generated files on first attempt"
        );
        return Ok(files);
    }

    info!(
        project = project_id.unwrap_or("-"),
        "LLM returned no file blocks on attempt 1 — sending reformat reminder"
    );

    // ── Attempt 2: Self-correction with previous output ──────────────────
    let response_preview = if response.len() > 2000 {
        format!("{}...[truncated]", &response[..2000])
    } else {
        response.clone()
    };

    let reformat_prompt = format!(
        r#"Your previous response did NOT follow the required output format. Here is what you produced:

---BEGIN YOUR PREVIOUS OUTPUT---
{response_preview}
---END YOUR PREVIOUS OUTPUT---

That output cannot be parsed. You MUST rewrite it using this EXACT format:

=== FILE: path/to/file.ext ===
[complete file contents]
=== END FILE ===

Rules:
- Start your response with `=== FILE:` as the VERY FIRST characters
- No markdown fences (no ```), no explanations, no commentary between files
- Every file must be complete — no stubs, no TODOs
- Generate ALL files the application needs

Original task:
{prompt_summary}

Now output the files correctly:"#,
        response_preview = response_preview,
        prompt_summary = &prompt[..prompt.len().min(3000)]
    );

    let mut response2 = super::chat::call_llm_simple_for_project(app, &reformat_prompt, project_id)
        .await
        .map_err(|e| format!("LLM reformat attempt 2 failed: {e}"))?;

    for cont_idx in 0..MAX_CONTINUATIONS {
        if !looks_truncated(&response2) {
            break;
        }
        info!(
            project = project_id.unwrap_or("-"),
            continuation = cont_idx + 1,
            "Reformat response truncated — continuing"
        );
        let continuation = super::chat::call_llm_simple_for_project(app, CONTINUE_PROMPT, project_id)
            .await
            .map_err(|e| format!("Continuation failed: {e}"))?;
        merge_continuation(&mut response2, &continuation);
    }

    let files2 = validate_and_clean(parse_file_blocks(&response2));
    if !files2.is_empty() {
        info!(
            project = project_id.unwrap_or("-"),
            file_count = files2.len(),
            "LLM generated files on reformat attempt 2"
        );
        return Ok(files2);
    }

    info!(
        project = project_id.unwrap_or("-"),
        "LLM returned no file blocks on attempt 2 — sending strict reformat demand"
    );

    // ── Attempt 3: Strict minimal prompt ─────────────────────────────────
    let strict_prompt = format!(
        r#"You are a code generator. Output complete application files.

MANDATORY FORMAT — every file must use:
=== FILE: relative/path/to/file ===
[full file content]
=== END FILE ===

Your response must start with `=== FILE:` — nothing before it.
No markdown. No commentary. Just files.

Build this application:
{prompt_summary}

Start now with === FILE:"#,
        prompt_summary = &prompt[..prompt.len().min(4000)]
    );

    let mut response3 = super::chat::call_llm_simple_for_project(app, &strict_prompt, project_id)
        .await
        .map_err(|e| format!("LLM strict attempt 3 failed: {e}"))?;

    for _cont_idx in 0..MAX_CONTINUATIONS {
        if !looks_truncated(&response3) {
            break;
        }
        let continuation = super::chat::call_llm_simple_for_project(app, CONTINUE_PROMPT, project_id)
            .await
            .map_err(|e| format!("Continuation failed: {e}"))?;
        merge_continuation(&mut response3, &continuation);
    }

    let files3 = validate_and_clean(parse_file_blocks(&response3));
    if !files3.is_empty() {
        info!(
            project = project_id.unwrap_or("-"),
            file_count = files3.len(),
            "LLM generated files on strict attempt 3"
        );
        return Ok(files3);
    }

    Err(
        "Code generation failed: the LLM did not produce any files after 3 attempts. \
         Please check your API key and model configuration, then try again."
            .to_string(),
    )
}

/// Call the LLM with an explicit provider/model and parse the generated files.
/// Same retry + continuation strategy as `generate_app_files`.
pub async fn generate_app_files_with_model(
    app: &Arc<AppState>,
    prompt: &str,
    provider: &str,
    model: &str,
) -> Result<Vec<(String, String)>, String> {
    use nexus_store::parse_file_blocks;

    let mut response = super::chat::call_llm_simple_with_model(app, prompt, provider, model)
        .await
        .map_err(|e| format!("LLM call failed: {e}"))?;

    for cont_idx in 0..MAX_CONTINUATIONS {
        if !looks_truncated(&response) {
            break;
        }
        info!(
            continuation = cont_idx + 1,
            model = model,
            response_len = response.len(),
            "Response truncated — sending continuation"
        );
        let continuation = super::chat::call_llm_simple_with_model(app, CONTINUE_PROMPT, provider, model)
            .await
            .map_err(|e| format!("Continuation {} failed: {e}", cont_idx + 1))?;
        merge_continuation(&mut response, &continuation);
    }

    let files = validate_and_clean(parse_file_blocks(&response));
    if !files.is_empty() {
        return Ok(files);
    }

    info!("LLM returned no file blocks with model {model} — retrying with self-correction");

    let response_preview = if response.len() > 2000 {
        format!("{}...[truncated]", &response[..2000])
    } else {
        response.clone()
    };

    let reformat_prompt = format!(
        r#"Your previous response did NOT follow the required format. Here is what you produced:

---BEGIN YOUR PREVIOUS OUTPUT---
{response_preview}
---END YOUR PREVIOUS OUTPUT---

Rewrite it using EXACTLY this format:

=== FILE: path/to/file.ext ===
[complete file contents]
=== END FILE ===

Start with `=== FILE:` immediately. No commentary. Generate ALL files.

Original task:
{prompt_summary}"#,
        response_preview = response_preview,
        prompt_summary = &prompt[..prompt.len().min(3000)]
    );

    let mut response2 = super::chat::call_llm_simple_with_model(app, &reformat_prompt, provider, model)
        .await
        .map_err(|e| format!("LLM reformat attempt failed: {e}"))?;

    for _cont_idx in 0..MAX_CONTINUATIONS {
        if !looks_truncated(&response2) {
            break;
        }
        let continuation = super::chat::call_llm_simple_with_model(app, CONTINUE_PROMPT, provider, model)
            .await
            .map_err(|e| format!("Continuation failed: {e}"))?;
        merge_continuation(&mut response2, &continuation);
    }

    let files2 = validate_and_clean(parse_file_blocks(&response2));
    if !files2.is_empty() {
        return Ok(files2);
    }

    Err(format!(
        "Code generation failed with model {model}: no files produced after 2 attempts. \
         Try a more capable model or check your API key."
    ))
}
