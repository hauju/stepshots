use serde::Serialize;

/// JSON output for `record` and `record --dry-run`.
#[derive(Serialize)]
pub struct RecordOutput {
    pub success: bool,
    pub command: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dry_run: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tutorials: Option<Vec<TutorialOutput>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorOutput>,
}

#[derive(Serialize)]
pub struct TutorialOutput {
    pub key: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    pub steps_total: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub steps_completed: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub steps: Option<Vec<StepOutput>>,
}

/// Per-step result.
#[derive(Serialize)]
pub struct StepOutput {
    pub index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// JSON output for `inspect --json`.
#[derive(Serialize)]
pub struct InspectOutput {
    pub url: String,
    pub elements: Vec<InspectElement>,
}

#[derive(Serialize)]
pub struct InspectElement {
    pub index: usize,
    pub tag: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element_type: Option<String>,
    pub selector: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aria_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub href: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds: Option<InspectBounds>,
}

#[derive(Serialize)]
pub struct InspectBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Structured error in JSON output.
#[derive(Serialize)]
pub struct ErrorOutput {
    pub category: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tutorial: Option<String>,
}
