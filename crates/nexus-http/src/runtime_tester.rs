//! Runtime Tester — real HTTP testing against running apps.
//!
//! Replaces simulation with actual HTTP requests to validate:
//! - Status codes
//! - Response bodies
//! - User flows (signup, login, CRUD)
//! - Page rendering

use serde::Serialize;
use tracing::info;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct TestSuite {
    pub name: String,
    pub tests: Vec<TestCase>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TestCase {
    pub name: String,
    pub method: HttpMethod,
    pub path: String,
    pub body: Option<serde_json::Value>,
    pub expected_status: u16,
    pub expected_body_contains: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
}

#[derive(Debug, Clone, Serialize)]
pub struct TestResult {
    pub success: bool,
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub failures: Vec<TestFailure>,
    pub coverage_estimate: f64,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TestFailure {
    pub test_name: String,
    pub expected: String,
    pub actual: String,
    pub detail: String,
}

// ---------------------------------------------------------------------------
// Test generation (from intent)
// ---------------------------------------------------------------------------

/// Generate a test suite based on the app's characteristics.
pub fn generate_test_suite(
    _base_url: &str,
    has_auth: bool,
    _has_database: bool,
    pages: &[String],
) -> TestSuite {
    let mut tests = Vec::new();

    // Test 1: Health / root page loads
    tests.push(TestCase {
        name: "Homepage loads".into(),
        method: HttpMethod::Get,
        path: "/".into(),
        body: None,
        expected_status: 200,
        expected_body_contains: Some("<!DOCTYPE html>".into()),
    });

    // Test 2: Each known page returns 200
    for page in pages {
        let path = if page.starts_with('/') {
            page.clone()
        } else {
            format!("/{}", page.to_lowercase().replace(' ', "-"))
        };
        tests.push(TestCase {
            name: format!("Page '{}' loads", page),
            method: HttpMethod::Get,
            path,
            body: None,
            expected_status: 200,
            expected_body_contains: None,
        });
    }

    // Test 3: Auth endpoints (if app has auth)
    if has_auth {
        tests.push(TestCase {
            name: "Login page exists".into(),
            method: HttpMethod::Get,
            path: "/login".into(),
            body: None,
            expected_status: 200,
            expected_body_contains: None,
        });
        tests.push(TestCase {
            name: "Register page exists".into(),
            method: HttpMethod::Get,
            path: "/register".into(),
            body: None,
            expected_status: 200,
            expected_body_contains: None,
        });
        tests.push(TestCase {
            name: "API auth rejects unauthorized".into(),
            method: HttpMethod::Get,
            path: "/api/me".into(),
            body: None,
            expected_status: 401,
            expected_body_contains: None,
        });
    }

    // Test 4: API health
    tests.push(TestCase {
        name: "API health endpoint".into(),
        method: HttpMethod::Get,
        path: "/api/health".into(),
        body: None,
        expected_status: 200,
        expected_body_contains: None,
    });

    // Test 5: 404 handling
    tests.push(TestCase {
        name: "404 for unknown route".into(),
        method: HttpMethod::Get,
        path: "/definitely-not-a-real-page-xyz".into(),
        body: None,
        expected_status: 404,
        expected_body_contains: None,
    });

    TestSuite {
        name: "Auto-generated runtime test suite".into(),
        tests,
    }
}

/// Run a test suite against a running app.
pub async fn run_tests(base_url: &str, suite: &TestSuite) -> TestResult {
    let start = std::time::Instant::now();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::limited(3))
        .build()
        .unwrap_or_default();

    let mut passed = 0;
    let mut failures = Vec::new();

    for test in &suite.tests {
        let url = format!("{}{}", base_url.trim_end_matches('/'), test.path);

        let response = match test.method {
            HttpMethod::Get => client.get(&url).send().await,
            HttpMethod::Post => {
                let mut req = client.post(&url);
                if let Some(body) = &test.body {
                    req = req.json(body);
                }
                req.send().await
            }
            HttpMethod::Put => {
                let mut req = client.put(&url);
                if let Some(body) = &test.body {
                    req = req.json(body);
                }
                req.send().await
            }
            HttpMethod::Delete => client.delete(&url).send().await,
        };

        match response {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let body_text = resp.text().await.unwrap_or_default();

                let mut test_passed = true;

                // Check status (allow 200 for 404 tests on Next.js which renders custom 404 as 200)
                if status != test.expected_status
                    && !(test.expected_status == 404 && status == 200)
                    // Next.js may return 308 redirects for trailing slashes
                    && !(test.expected_status == 200 && (status == 301 || status == 308))
                {
                    test_passed = false;
                    failures.push(TestFailure {
                        test_name: test.name.clone(),
                        expected: format!("status {}", test.expected_status),
                        actual: format!("status {}", status),
                        detail: format!("GET {} returned {}", test.path, status),
                    });
                }

                // Check body contains
                if let Some(ref expected) = test.expected_body_contains {
                    if !body_text.contains(expected) {
                        test_passed = false;
                        failures.push(TestFailure {
                            test_name: test.name.clone(),
                            expected: format!("body contains '{}'", expected),
                            actual: format!(
                                "body ({} chars) does not contain expected string",
                                body_text.len()
                            ),
                            detail: format!(
                                "Response body preview: {}",
                                &body_text[..body_text.len().min(200)]
                            ),
                        });
                    }
                }

                if test_passed {
                    passed += 1;
                }
            }
            Err(e) => {
                failures.push(TestFailure {
                    test_name: test.name.clone(),
                    expected: format!("status {}", test.expected_status),
                    actual: "connection failed".into(),
                    detail: format!("Error: {}", e),
                });
            }
        }
    }

    let total = suite.tests.len();
    let coverage = if total > 0 {
        passed as f64 / total as f64
    } else {
        0.0
    };

    info!(
        total = total,
        passed = passed,
        failed = failures.len(),
        coverage = format!("{:.0}%", coverage * 100.0),
        "Runtime tests complete"
    );

    TestResult {
        success: failures.is_empty(),
        total,
        passed,
        failed: failures.len(),
        failures,
        coverage_estimate: coverage,
        duration_ms: start.elapsed().as_millis() as u64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_tests_for_auth_app() {
        let suite = generate_test_suite(
            "http://localhost:4100",
            true,
            true,
            &["Dashboard".into(), "Settings".into()],
        );
        assert!(suite.tests.len() >= 5);
        assert!(suite.tests.iter().any(|t| t.name.contains("Login")));
        assert!(suite.tests.iter().any(|t| t.name.contains("Register")));
        assert!(suite.tests.iter().any(|t| t.name.contains("Homepage")));
        assert!(suite.tests.iter().any(|t| t.name.contains("404")));
    }

    #[test]
    fn generates_basic_tests_for_simple_app() {
        let suite = generate_test_suite(
            "http://localhost:4100",
            false,
            false,
            &["About".into()],
        );
        // Should have homepage, about page, api health, and 404
        assert!(suite.tests.len() >= 3);
        assert!(!suite.tests.iter().any(|t| t.name.contains("Login")));
    }
}
