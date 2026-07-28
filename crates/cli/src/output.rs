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
    /// Annotations that could not be anchored when this step was recorded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotation_warnings: Option<Vec<AnnotationWarning>>,
}

/// An annotation that failed to anchor onto a recorded step.
#[derive(Serialize)]
pub struct AnnotationWarning {
    /// Which annotation type drifted: highlight | blur | arrow | hotspot | popup | zoom.
    pub kind: &'static str,
    /// The CSS selector that was supposed to anchor it.
    pub selector: String,
    /// Why it drifted: "orphaned" (element no longer exists) or "off_screen".
    pub reason: &'static str,
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

/// JSON output for `upload --json`.
#[derive(Serialize)]
pub struct UploadOutput {
    pub success: bool,
    pub command: &'static str,
    pub demos: Vec<UploadedDemo>,
}

#[derive(Serialize)]
pub struct UploadedDemo {
    pub demo_id: String,
    pub view_url: String,
    /// True when an existing demo was replaced via --demo-id.
    pub replaced: bool,
}

/// JSON output for `list --json`.
#[derive(Serialize)]
pub struct ListOutput {
    pub success: bool,
    pub command: &'static str,
    pub config: String,
    pub tutorials: Vec<ListTutorial>,
}

#[derive(Serialize)]
pub struct ListTutorial {
    pub key: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub steps: usize,
}

/// JSON output for `verify --json`.
#[derive(Serialize)]
pub struct VerifyOutput {
    /// False when drift was found at or above the `--fail-on` level.
    pub success: bool,
    pub command: &'static str,
    pub config: String,
    pub summary: VerifySummary,
    pub tutorials: Vec<VerifyTutorial>,
}

#[derive(Serialize)]
pub struct VerifySummary {
    pub tutorials: usize,
    pub ok: usize,
    pub warn: usize,
    pub fail: usize,
    pub steps_checked: usize,
}

#[derive(Serialize)]
pub struct VerifyTutorial {
    pub key: String,
    pub title: String,
    /// "ok", "warn" (annotation drift only), or "fail" (a step could not execute).
    pub status: &'static str,
    /// Set when the tutorial's start page failed to load; all steps are skipped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<VerifyFailure>,
    pub steps: Vec<VerifyStep>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub annotation_warnings: Vec<VerifyAnnotationWarning>,
}

/// Per-step verification result.
#[derive(Serialize)]
pub struct VerifyStep {
    pub index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    /// "ok", "fail", or "skipped" (an earlier step failed, page state unknown).
    pub status: &'static str,
    #[serde(flatten)]
    pub failure: Option<VerifyFailure>,
}

/// Why and where a verification check failed, plus what to do about it.
#[derive(Serialize)]
pub struct VerifyFailure {
    /// "orphaned" (selector matched nothing), "navigation_failed", or "action_failed".
    pub reason: &'static str,
    pub message: String,
    /// URL the page was on when the check failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_url: Option<String>,
    /// Screenshot of the page as it looked at failure time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screenshot: Option<String>,
    /// Suggested next command to diagnose or repair the drift.
    pub hint: String,
}

/// An annotation whose anchor drifted during verification. Warn-level:
/// the demo still records, but the annotation would be dropped.
#[derive(Serialize)]
pub struct VerifyAnnotationWarning {
    /// 0-based index of the step the annotation belongs to (matches `steps[].index`).
    pub step: usize,
    /// Which annotation type drifted: highlight | blur | arrow | hotspot | popup | zoom.
    pub kind: &'static str,
    pub selector: String,
    /// "orphaned" (element no longer exists) or "off_screen".
    pub reason: &'static str,
}

/// JSON output for `doctor --json`.
#[derive(Serialize)]
pub struct DoctorOutput {
    pub success: bool,
    pub command: &'static str,
    pub version: &'static str,
    pub checks: Vec<DoctorCheck>,
}

/// A single doctor check result.
#[derive(Serialize)]
pub struct DoctorCheck {
    pub name: &'static str,
    /// "ok", "warn" (works for some flows), or "fail" (broken).
    pub status: &'static str,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

/// Top-level result of `stepshots drift`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriftOutput {
    pub success: bool,
    pub command: &'static str,
    /// Worst verdict across every asset checked: ok | drifted | stale.
    pub verdict: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assets: Option<Vec<DriftAsset>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorOutput>,
}

/// One bundle or tour file checked.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriftAsset {
    pub name: String,
    /// ok | drifted | stale | unsupported (no DOM extracts in the bundle)
    pub verdict: String,
    pub routes: Vec<DriftRoute>,
    /// Steps targeting an element that captured no durable anchor, so drift
    /// against them cannot be detected by any means.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unanchored: Vec<String>,
}

/// One distinct route checked. Steps sharing a `current_path` collapse into a
/// single route — they show the same page.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriftRoute {
    pub path: String,
    pub url: String,
    /// 1-based index of the step whose extract was used as the baseline.
    pub first_step: usize,
    pub verdict: String,
    pub summary: DriftSummary,
    pub findings: Vec<crate::drift::Finding>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriftSummary {
    pub critical: usize,
    pub content: usize,
    /// Collapsed layout groups, not individual moved nodes.
    pub layout: usize,
    pub nodes_before: usize,
    pub nodes_after: usize,
}
