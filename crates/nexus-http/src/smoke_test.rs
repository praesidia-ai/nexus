//! End-to-end smoke test runner — validates generated apps actually work.
//!
//! Starts from a set of known routes and verifies that each one returns
//! a successful HTTP response.  Also checks that the homepage references
//! static assets (CSS/JS), catching broken builds that serve empty shells.

use serde::Serialize;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct SmokeTestReport {
    pub passed: bool,
    pub tests: Vec<SmokeTestResult>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SmokeTestResult {
    pub name: String,
    pub passed: bool,
    pub status_code: Option<u16>,
    pub duration_ms: u64,
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// Runner
// ---------------------------------------------------------------------------

pub struct SmokeTestRunner {
    client: reqwest::Client,
}

impl Default for SmokeTestRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl SmokeTestRunner {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Run smoke tests against a running application.
    ///
    /// `app_url` — base URL such as `http://localhost:3000`.
    /// `pages`   — logical page names (e.g. `["Home", "About", "Dashboard"]`).
    pub async fn run(&self, app_url: &str, pages: &[String]) -> SmokeTestReport {
        let mut results = Vec::new();

        // Test 1: Homepage loads
        results.push(self.test_url(app_url, "/", "homepage").await);

        // Test 2: Each suggested page loads
        for page in pages {
            let route = if page.eq_ignore_ascii_case("home") {
                "/".to_string()
            } else {
                format!("/{}", page.to_lowercase().replace(' ', "-"))
            };

            // Skip duplicate homepage test
            if route == "/" {
                continue;
            }

            results.push(
                self.test_url(app_url, &route, &format!("page_{}", page))
                    .await,
            );
        }

        // Test 3: Static assets (check for CSS/JS references in homepage HTML)
        results.push(self.test_static_assets(app_url).await);

        SmokeTestReport {
            passed: results.iter().all(|r| r.passed),
            tests: results,
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }

    async fn test_url(&self, base: &str, path: &str, name: &str) -> SmokeTestResult {
        let url = format!("{}{}", base.trim_end_matches('/'), path);
        let start = std::time::Instant::now();

        match self
            .client
            .get(&url)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
        {
            Ok(resp) => {
                let status = resp.status();
                SmokeTestResult {
                    name: name.to_string(),
                    passed: status.is_success(),
                    status_code: Some(status.as_u16()),
                    duration_ms: start.elapsed().as_millis() as u64,
                    error: if !status.is_success() {
                        Some(format!("HTTP {}", status))
                    } else {
                        None
                    },
                }
            }
            Err(e) => SmokeTestResult {
                name: name.to_string(),
                passed: false,
                status_code: None,
                duration_ms: start.elapsed().as_millis() as u64,
                error: Some(e.to_string()),
            },
        }
    }

    async fn test_static_assets(&self, base: &str) -> SmokeTestResult {
        let start = std::time::Instant::now();

        match self.client.get(base).send().await {
            Ok(resp) => {
                let body = resp.text().await.unwrap_or_default();
                let has_css = body.contains(".css") || body.contains("<style");
                let has_js = body.contains(".js") || body.contains("<script");

                SmokeTestResult {
                    name: "static_assets".into(),
                    passed: has_css || has_js,
                    status_code: Some(200),
                    duration_ms: start.elapsed().as_millis() as u64,
                    error: if !has_css && !has_js {
                        Some("No CSS or JS references found in homepage HTML".into())
                    } else {
                        None
                    },
                }
            }
            Err(e) => SmokeTestResult {
                name: "static_assets".into(),
                passed: false,
                status_code: None,
                duration_ms: start.elapsed().as_millis() as u64,
                error: Some(e.to_string()),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runner_creates_without_panic() {
        let _runner = SmokeTestRunner::new();
    }

    #[test]
    fn report_serializes() {
        let report = SmokeTestReport {
            passed: true,
            tests: vec![SmokeTestResult {
                name: "homepage".into(),
                passed: true,
                status_code: Some(200),
                duration_ms: 42,
                error: None,
            }],
            timestamp: "2026-04-04T00:00:00Z".into(),
        };
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("homepage"));
    }
}
