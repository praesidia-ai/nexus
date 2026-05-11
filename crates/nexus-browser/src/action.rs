use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BrowserAction {
    Navigate { url: String },
    Click { selector: String },
    Type { selector: String, text: String },
    Fill { selector: String, value: String },
    SelectOption { selector: String, value: String },
    Screenshot { path: Option<String>, full_page: bool },
    WaitForSelector { selector: String, timeout_ms: u64 },
    WaitForNavigation { timeout_ms: u64 },
    GetText { selector: String },
    GetAttribute { selector: String, attribute: String },
    Evaluate { script: String },
    ScrollTo { selector: String },
    Hover { selector: String },
    GoBack,
    GoForward,
    Reload,
    Close,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    pub action: String,
    pub success: bool,
    pub output: Option<String>,
    pub screenshot: Option<String>,
    pub duration_ms: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageState {
    pub url: String,
    pub title: String,
    pub dom_snapshot: Option<String>,
    pub visible_text: String,
    pub links: Vec<PageLink>,
    pub forms: Vec<PageForm>,
    pub interactive_elements: Vec<InteractiveElement>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageLink {
    pub text: String,
    pub href: String,
    pub selector: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageForm {
    pub action: String,
    pub method: String,
    pub fields: Vec<FormField>,
    pub selector: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormField {
    pub name: String,
    pub field_type: String,
    pub selector: String,
    pub required: bool,
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractiveElement {
    pub tag: String,
    pub text: String,
    pub selector: String,
    pub element_type: String,
}
