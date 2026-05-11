use crate::action::BrowserAction;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct E2eTest {
    pub name: String,
    pub description: String,
    pub steps: Vec<TestStep>,
    pub assertions: Vec<Assertion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestStep {
    pub action: BrowserAction,
    pub description: String,
    pub screenshot_after: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Assertion {
    UrlContains { substring: String },
    TextVisible { text: String },
    ElementExists { selector: String },
    ElementNotExists { selector: String },
    AttributeEquals { selector: String, attribute: String, value: String },
    PageTitleContains { text: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub test_name: String,
    pub passed: bool,
    pub steps_completed: usize,
    pub total_steps: usize,
    pub failed_assertion: Option<String>,
    pub screenshots: Vec<String>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct E2eSuite {
    pub name: String,
    pub base_url: String,
    pub tests: Vec<E2eTest>,
}

impl E2eSuite {
    /// Generate a basic smoke test suite for a web application.
    pub fn smoke_test(base_url: &str) -> Self {
        Self {
            name: "Smoke Tests".to_string(),
            base_url: base_url.to_string(),
            tests: vec![
                E2eTest {
                    name: "Homepage loads".to_string(),
                    description: "Verify the homepage loads successfully".to_string(),
                    steps: vec![TestStep {
                        action: BrowserAction::Navigate {
                            url: base_url.to_string(),
                        },
                        description: "Navigate to homepage".to_string(),
                        screenshot_after: true,
                    }],
                    assertions: vec![
                        Assertion::ElementExists {
                            selector: "body".to_string(),
                        },
                        Assertion::PageTitleContains {
                            text: String::new(),
                        },
                    ],
                },
                E2eTest {
                    name: "Health check responds".to_string(),
                    description: "Verify the health endpoint returns OK".to_string(),
                    steps: vec![TestStep {
                        action: BrowserAction::Navigate {
                            url: format!("{base_url}/api/health"),
                        },
                        description: "Navigate to health endpoint".to_string(),
                        screenshot_after: false,
                    }],
                    assertions: vec![Assertion::TextVisible {
                        text: "ok".to_string(),
                    }],
                },
            ],
        }
    }

    /// Generate auth flow tests.
    pub fn auth_tests(base_url: &str) -> Self {
        Self {
            name: "Auth Flow Tests".to_string(),
            base_url: base_url.to_string(),
            tests: vec![
                E2eTest {
                    name: "Login page loads".to_string(),
                    description: "Verify login page is accessible".to_string(),
                    steps: vec![TestStep {
                        action: BrowserAction::Navigate {
                            url: format!("{base_url}/login"),
                        },
                        description: "Navigate to login".to_string(),
                        screenshot_after: true,
                    }],
                    assertions: vec![
                        Assertion::ElementExists {
                            selector: "input[type=\"email\"]".to_string(),
                        },
                        Assertion::ElementExists {
                            selector: "input[type=\"password\"]".to_string(),
                        },
                    ],
                },
                E2eTest {
                    name: "Signup page loads".to_string(),
                    description: "Verify signup page is accessible".to_string(),
                    steps: vec![TestStep {
                        action: BrowserAction::Navigate {
                            url: format!("{base_url}/signup"),
                        },
                        description: "Navigate to signup".to_string(),
                        screenshot_after: true,
                    }],
                    assertions: vec![Assertion::ElementExists {
                        selector: "form".to_string(),
                    }],
                },
            ],
        }
    }
}
