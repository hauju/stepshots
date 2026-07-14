use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ============================================================================
// Recording config types (stepshots.config.json — shared across CLI, extension, etc.)
// ============================================================================

/// Canonical URL of the published JSON Schema for `stepshots.config.json`.
/// Written as the `$schema` key by `stepshots init` and config exports so
/// editors provide autocomplete and validation.
pub const CONFIG_SCHEMA_URL: &str =
    "https://raw.githubusercontent.com/hauju/stepshots/main/schema/stepshots.config.schema.json";

/// Step actions accepted in `stepshots.config.json`.
pub const VALID_ACTIONS: &[&str] = &[
    "click",
    "type",
    "key",
    "scroll",
    "scroll-to",
    "hover",
    "navigate",
    "wait",
    "select",
];

/// Callout placements accepted for highlights and hotspots.
pub const VALID_POSITIONS: &[&str] = &["top", "bottom", "left", "right"];

#[cfg(feature = "schema")]
fn action_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "string",
        "enum": VALID_ACTIONS,
        "description": "What this step does: click, type, key, scroll, scroll-to, hover, navigate, wait, or select."
    })
}

#[cfg(feature = "schema")]
fn position_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "string",
        "enum": VALID_POSITIONS,
        "description": "Where the callout is placed relative to the element: top, bottom, left, or right."
    })
}

#[cfg(feature = "schema")]
fn theme_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "string",
        "enum": ["light", "dark"],
        "description": "Color scheme forced during recording. Defaults to the browser default."
    })
}

/// Top-level config from `stepshots.config.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct StepshotsConfig {
    /// JSON Schema reference for editor autocomplete and validation.
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    /// Base URL prepended to every relative tutorial and step URL, e.g. "https://app.example.com".
    pub base_url: String,
    /// Browser viewport in CSS pixels. Ignored when `format` names a preset.
    #[serde(default = "default_viewport")]
    pub viewport: Viewport,
    /// Named viewport preset (e.g. "desktop", "mobile"). Overrides `viewport` unless "custom".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<DemoFormat>,
    /// Milliseconds to wait after each action before capturing, unless a step sets its own `delay`.
    #[serde(default = "default_delay")]
    pub default_delay: u64,
    /// Color scheme for recording: "dark" or "light". Defaults to browser default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "schema", schemars(schema_with = "theme_schema"))]
    pub theme: Option<String>,
    /// Tutorials to record, keyed by a short slug that becomes the output file name.
    pub tutorials: HashMap<String, TutorialConfig>,
}

/// A single tutorial within the config.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct TutorialConfig {
    /// Start page for the tutorial — absolute, or relative to the config `baseUrl`.
    pub url: String,
    /// Human-readable title, shown in the dashboard and used as the demo title on upload.
    pub title: String,
    /// Optional longer description of what the tutorial shows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Ordered actions to perform; each step becomes one screenshot in the demo.
    pub steps: Vec<StepConfig>,
}

/// A single step action in a tutorial.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct StepConfig {
    #[cfg_attr(feature = "schema", schemars(schema_with = "action_schema"))]
    pub action: String,
    /// Optional label for the step, shown in progress output and the dashboard.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// CSS selector of the element this action targets. Required for click, type, hover, and select.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    /// How the selector was derived (informational, set by recording tools).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selector_quality: Option<String>,
    /// Text to enter. Required for the "type" action. Supports `${ENV_VAR}` interpolation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Target URL for the "navigate" action — absolute, or relative to `baseUrl`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Key to press for the "key" action, e.g. "Enter" or "Escape".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// Option value to choose for the "select" action.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Milliseconds to wait after this action, overriding the config `defaultDelay`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay: Option<u64>,
    /// Horizontal scroll distance in pixels for the "scroll" action.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scroll_x: Option<f64>,
    /// Vertical scroll distance in pixels for the "scroll" action.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scroll_y: Option<f64>,
    /// Horizontal scroll position the page is restored to before this step's capture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene_scroll_x: Option<f64>,
    /// Vertical scroll position the page is restored to before this step's capture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene_scroll_y: Option<f64>,
    /// Optional CSS selector used only for resolving highlight bounds.
    /// When set, overrides `selector` for highlight resolution so that
    /// action steps (scroll, navigate) can target one element while
    /// highlighting another.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highlight_selector: Option<String>,
    /// Highlight annotations drawn on this step's screenshot.
    #[serde(alias = "annotations")]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub highlights: Vec<HighlightConfig>,
    /// Regions blurred on this step's screenshot (e.g. to hide sensitive data).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blur_regions: Vec<BlurConfig>,
    /// Arrows drawn between two elements on this step's screenshot.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arrows: Vec<ArrowConfig>,
    /// Pulsing hotspot markers placed on elements.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hotspots: Vec<HotspotConfig>,
    /// Popup cards anchored to elements.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub popups: Vec<PopupConfig>,
    /// Regions the viewer zooms into on this step.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub zoom_regions: Vec<ZoomConfig>,
}

/// Highlight config for a step in the recording config.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct HighlightConfig {
    /// Explicit pixel bounds. When set, wins over selector resolution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<ElementBounds>,
    /// Draw a border around the highlighted element (defaults to true).
    #[serde(alias = "highlight")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_border: Option<bool>,
    /// Text shown in a callout next to the highlight.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "schema", schemars(schema_with = "position_schema"))]
    pub position: Option<String>,
    /// Highlight color as a CSS color value, e.g. "#7c3aed".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Draw an arrow from the callout to the element.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arrow: Option<bool>,
}

/// Blur region config — resolved from CSS selector to element bounds.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct BlurConfig {
    /// CSS selector of the element to blur.
    pub selector: String,
}

/// Arrow config — resolved from two CSS selectors to element center points.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct ArrowConfig {
    /// CSS selector of the element the arrow starts from.
    pub from_selector: String,
    /// CSS selector of the element the arrow points to.
    pub to_selector: String,
    /// Arrow color as a CSS color value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Stroke width in pixels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke_width: Option<f64>,
    /// Curve amount from -1.0 (bend left) to 1.0 (bend right); 0 is straight.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curvature: Option<f64>,
}

/// Hotspot config — resolved from CSS selector to element center point.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct HotspotConfig {
    /// CSS selector of the element the hotspot is centered on.
    pub selector: String,
    /// Text shown in a callout next to the hotspot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "schema", schemars(schema_with = "position_schema"))]
    pub position: Option<String>,
    /// Hotspot color as a CSS color value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Hotspot diameter in pixels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<f64>,
    /// Whether clicking the hotspot advances the demo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_click_target: Option<bool>,
}

/// Popup config — resolved from CSS selector to element center point.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct PopupConfig {
    /// CSS selector of the element the popup is anchored to.
    pub selector: String,
    /// Popup heading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Popup body text.
    pub body: String,
    /// Popup width in pixels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    /// Background color as a CSS color value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Text color as a CSS color value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_color: Option<String>,
    /// Rendering style: "card" (default) or "button".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    /// Visual variant: "primary", "secondary", "ghost", etc.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    /// Size token: "xs", "sm", "md", "lg".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    /// Optional button label displayed at the bottom of the popup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub button_text: Option<String>,
    /// Optional URL the button links to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub button_url: Option<String>,
    /// Whether the button opens in a new tab.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_in_new_tab: Option<bool>,
}

/// Zoom region config — resolved from CSS selector to element bounds.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct ZoomConfig {
    /// CSS selector of the element to zoom into.
    pub selector: String,
    /// Zoom factor, e.g. 2.0 for 2x.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub magnification: Option<f64>,
    /// Zoom animation start delay in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay: Option<u32>,
    /// Zoom animation duration in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<u32>,
}

/// Preset format for demo recording viewport dimensions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case")]
pub enum DemoFormat {
    DesktopHd,
    Desktop,
    TabletLandscape,
    TabletPortrait,
    Mobile,
    MobileLandscape,
    Square,
    Custom,
}

impl DemoFormat {
    /// Returns the pixel dimensions for this preset, or `None` for `Custom`.
    pub fn dimensions(&self) -> Option<(u32, u32)> {
        match self {
            Self::DesktopHd => Some((1920, 1080)),
            Self::Desktop => Some((1280, 800)),
            Self::TabletLandscape => Some((1024, 768)),
            Self::TabletPortrait => Some((768, 1024)),
            Self::Mobile => Some((390, 844)),
            Self::MobileLandscape => Some((844, 390)),
            Self::Square => Some((1080, 1080)),
            Self::Custom => None,
        }
    }

    /// Human-readable label for display.
    pub fn label(&self) -> &str {
        match self {
            Self::DesktopHd => "Desktop HD",
            Self::Desktop => "Desktop",
            Self::TabletLandscape => "Tablet Landscape",
            Self::TabletPortrait => "Tablet Portrait",
            Self::Mobile => "Mobile",
            Self::MobileLandscape => "Mobile Landscape",
            Self::Square => "Square",
            Self::Custom => "Custom",
        }
    }

    /// Infer the format from a viewport's dimensions. Returns `Custom` if no preset matches.
    pub fn from_viewport(vp: &Viewport) -> Self {
        let dims = (vp.width, vp.height);
        [
            Self::DesktopHd,
            Self::Desktop,
            Self::TabletLandscape,
            Self::TabletPortrait,
            Self::Mobile,
            Self::MobileLandscape,
            Self::Square,
        ]
        .into_iter()
        .find(|f| f.dimensions() == Some(dims))
        .unwrap_or(Self::Custom)
    }

    /// All named presets (excludes `Custom`).
    pub fn all_presets() -> &'static [DemoFormat] {
        &[
            Self::DesktopHd,
            Self::Desktop,
            Self::TabletLandscape,
            Self::TabletPortrait,
            Self::Mobile,
            Self::MobileLandscape,
            Self::Square,
        ]
    }
}

impl std::fmt::Display for DemoFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// Resolve effective viewport dimensions: named format wins if present, otherwise raw viewport.
pub fn resolve_viewport(format: Option<&DemoFormat>, viewport: &Viewport) -> Viewport {
    match format.and_then(|f| f.dimensions()) {
        Some((w, h)) => Viewport {
            width: w,
            height: h,
            device_scale_factor: viewport.device_scale_factor,
        },
        None => viewport.clone(),
    }
}

pub fn default_viewport() -> Viewport {
    Viewport {
        width: 1280,
        height: 800,
        device_scale_factor: None,
    }
}

pub fn default_delay() -> u64 {
    500
}

// ============================================================================
// Bundle manifest types (.stepshot zip — shared across CLI, extension, server)
// ============================================================================

/// Highlight overlay for a step (highlight area + callout + styling).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HighlightEntry {
    pub bounds: ElementBounds,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arrow: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_click_target: Option<bool>,
    /// User-edited callout offset relative to element bounds (set by editor).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callout_offset: Option<CalloutOffset>,
    /// Whether this annotation was manually edited by the user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_edited: Option<bool>,
    /// Callout rendering style: "label" (default) or "card" (tooltip card with pointer).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callout_style: Option<String>,
    /// Button text for card-style callouts (defaults to "Next" if not set).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub button_text: Option<String>,
    /// Dim the rest of the screenshot to spotlight this highlight (defaults to true).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spotlight: Option<bool>,
    /// Opacity of the spotlight darkening overlay (0.0–1.0, defaults to 0.4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spotlight_opacity: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub animation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<u32>,
    /// Paint-order index within the step. Higher values render on top.
    /// Dashboard editor assigns this on first load for bundles that predate the field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub z_index: Option<u32>,
}

/// Callout position offset relative to the element bounds.
/// Values are percentages of viewport dimensions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalloutOffset {
    /// Horizontal offset from element center (% of viewport width).
    pub dx: f64,
    /// Vertical offset from element bottom (% of viewport height).
    pub dy: f64,
}

/// Pixel coordinates for a highlight region.
///
/// The optional `z_index` is only meaningful when `ElementBounds` is used directly
/// as an overlay (e.g. `BundleManifestStep.blur_regions`). When nested as a sub-`bounds`
/// field on another overlay (e.g. `HighlightEntry.bounds`, `ZoomRegion.bounds`), the
/// overlay's own `z_index` governs paint order and this field stays `None`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ElementBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub z_index: Option<u32>,
}

impl ElementBounds {
    /// True when this rect and `other` share a positive-area overlap.
    /// Axis-aligned; edges that merely touch (zero-area contact) count as
    /// non-intersecting.
    pub fn intersects(&self, other: &ElementBounds) -> bool {
        self.x < other.x + other.width
            && other.x < self.x + self.width
            && self.y < other.y + other.height
            && other.y < self.y + self.height
    }
}

/// A 2D point in viewport coordinates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Point2D {
    pub x: f64,
    pub y: f64,
}

/// An arrow pointer between two points.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArrowPointer {
    pub from: Point2D,
    pub to: Point2D,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curvature: Option<f64>,
    /// Optional text label displayed near the arrow's start (from) point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Font size in viewport pixels for the text label (defaults to 14).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f64>,
    /// Animation preset: "fade", "fade-up", "zoom-in", "pulse", or "none".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub animation: Option<String>,
    /// Animation start delay in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay: Option<u32>,
    /// Animation duration in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<u32>,
    /// Paint-order index within the step. Higher values render on top.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub z_index: Option<u32>,
}

/// A hotspot indicator at a specific point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HotspotIndicator {
    pub x: f64,
    pub y: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_click_target: Option<bool>,
    /// Paint-order index within the step. Higher values render on top.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub z_index: Option<u32>,
}

/// A popup card indicator at a specific point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PopupIndicator {
    pub x: f64,
    pub y: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border_radius: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub animation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dismissible: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_click_target: Option<bool>,
    /// Optional button label displayed at the bottom of the popup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub button_text: Option<String>,
    /// Optional URL the button links to (opens in new tab).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub button_url: Option<String>,
    /// Rendering style: "card" (default, tooltip card) or "button" (inline button).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    /// Visual variant: "primary", "secondary", "ghost", etc.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    /// Size token: "xs", "sm", "md", "lg".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    /// Whether clicking the popup's button opens in a new tab.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_in_new_tab: Option<bool>,
    /// Paint-order index within the step. Higher values render on top.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub z_index: Option<u32>,
}

/// A zoom region that triggers an animated zoom-into-area effect on the screenshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ZoomRegion {
    pub bounds: ElementBounds,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub magnification: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<u32>,
    /// Paint-order index within the step. Higher values render on top.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub z_index: Option<u32>,
}

/// Viewport dimensions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct Viewport {
    /// Width in CSS pixels.
    pub width: u32,
    /// Height in CSS pixels.
    pub height: u32,
    /// Device pixel ratio, e.g. 2.0 for retina-quality screenshots.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_scale_factor: Option<f64>,
}

/// Manifest format inside a `.stepshot` bundle zip.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleManifest {
    pub version: u32,
    pub viewport: Viewport,
    #[serde(alias = "baseUrl")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(alias = "startPath")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<DemoFormat>,
    pub steps: Vec<BundleManifestStep>,
}

// ---------------------------------------------------------------------------
// Live guided-tour track
//
// A recorded flow already carries everything a live "light the way" tour needs:
// each step's `selector`, `action`, `name`, highlight `callout`, and the
// `target_text`/`target_aria` drift anchors. `BundleManifest::to_tour_track`
// projects that onto the player's track schema, so ONE recording drives both a
// screenshot demo AND a live guided walkthrough. Shared by the CLI's
// `tour export` and the dashboard's hosted-tour endpoint.
// ---------------------------------------------------------------------------

/// How a live-tour step advances to the next. Serializes as `{ "type": "click" }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TourAdvance {
    /// Advance when the user clicks inside the target element.
    Click,
    /// Advance when the user types a non-empty value into the target element.
    Input,
}

impl TourAdvance {
    /// Map a recorded action to how the live step advances. Only interactive
    /// actions become tour steps; `navigate`/`wait`/etc. are setup and yield `None`.
    pub fn from_action(action: &str) -> Option<Self> {
        match action {
            "click" => Some(Self::Click),
            "type" => Some(Self::Input),
            _ => None,
        }
    }

    /// The player-facing advance kind string (`"click"` / `"input"`).
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Click => "click",
            Self::Input => "input",
        }
    }
}

/// Resilient anchors the player falls back to when `selector` no longer matches
/// the live DOM (UI drift). Captured from the target element at record time.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TourFallback {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aria: Option<String>,
}

impl TourFallback {
    /// A fallback carrying no anchor at all — nothing worth serializing.
    pub fn is_empty(&self) -> bool {
        self.text.is_none() && self.aria.is_none()
    }
}

/// One step of a live tour: spotlight `selector`, show `title`/`body`, advance on interaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TourStep {
    pub selector: String,
    pub title: String,
    pub body: String,
    pub advance: TourAdvance,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<TourFallback>,
}

/// An ordered set of tour steps — the data contract consumed by the `@stepshots/tour` player.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TourTrack {
    pub steps: Vec<TourStep>,
}

impl BundleManifest {
    /// Project this recording into a live guided-tour track. A step is included
    /// only if it carries a highlight callout AND an interactive (click/type)
    /// action; setup steps (navigate/wait, or callout-less steps) are dropped —
    /// the same convention that lets one recording serve both demo and tour.
    pub fn to_tour_track(&self) -> TourTrack {
        let steps = self
            .steps
            .iter()
            .filter_map(|s| {
                // First highlight that actually carries a callout — not just
                // index 0, since the editor can add/reorder highlights and a
                // decorative (callout-less) one may sit first.
                let callout_highlight = s
                    .highlights
                    .as_ref()?
                    .iter()
                    .find(|h| h.callout.is_some())?;
                let body = callout_highlight.callout.clone()?;
                let advance = TourAdvance::from_action(s.action.as_deref()?)?;
                let selector = s.selector.clone()?;
                // Owners redact sensitive areas with blur regions, which the
                // dashboard bakes into the served screenshot. If any blur covers
                // the callout-bearing highlight, the element's record-time text
                // must not resurface as plaintext here — drop the fallback
                // anchors so the public tour JSON stays redacted too. Degenerate
                // (zero-area) blur regions redact nothing and are ignored.
                let blurred = s.blur_regions.as_ref().is_some_and(|regions| {
                    regions
                        .iter()
                        .filter(|r| r.width > 0.0 && r.height > 0.0)
                        .any(|r| r.intersects(&callout_highlight.bounds))
                });
                let fallback = if blurred {
                    None
                } else {
                    let fallback = TourFallback {
                        text: s.target_text.clone(),
                        aria: s.target_aria.clone(),
                    };
                    (!fallback.is_empty()).then_some(fallback)
                };
                Some(TourStep {
                    selector,
                    title: s.name.clone().unwrap_or_default(),
                    body,
                    advance,
                    fallback,
                })
            })
            .collect();
        TourTrack { steps }
    }
}

/// A single step entry in the bundle manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleManifestStep {
    pub file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(alias = "currentPath")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_path: Option<String>,
    #[serde(alias = "targetUrl")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_url: Option<String>,
    #[serde(default)]
    pub selector: Option<String>,
    #[serde(alias = "selectorQuality")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selector_quality: Option<String>,
    /// Trimmed text content of the target element at record time. A resilient
    /// fallback anchor for the live-tour player when the CSS selector no longer
    /// matches (UI drift). Captured only for element-targeting actions.
    #[serde(alias = "targetText")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_text: Option<String>,
    /// aria-label of the target element at record time (fallback anchor).
    #[serde(alias = "targetAria")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_aria: Option<String>,
    #[serde(alias = "annotations")]
    #[serde(default)]
    pub highlights: Option<Vec<HighlightEntry>>,
    // Editor overlay types
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blur_regions: Option<Vec<ElementBounds>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arrows: Option<Vec<ArrowPointer>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hotspots: Option<Vec<HotspotIndicator>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub popups: Option<Vec<PopupIndicator>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zoom_regions: Option<Vec<ZoomRegion>>,
    // Action parameters (makes bundles self-contained for replay)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(alias = "scrollX")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scroll_x: Option<f64>,
    #[serde(alias = "scrollY")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scroll_y: Option<f64>,
    #[serde(alias = "sceneScrollX")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene_scroll_x: Option<f64>,
    #[serde(alias = "sceneScrollY")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene_scroll_y: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay: Option<u64>,
    /// Filenames of intermediate JPEG frames for flipbook-style scroll transitions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition_frames: Option<Vec<String>>,
}

impl From<&BundleManifestStep> for StepConfig {
    fn from(step: &BundleManifestStep) -> Self {
        StepConfig {
            action: step.action.clone().unwrap_or_default(),
            name: step.name.clone(),
            selector: step.selector.clone(),
            selector_quality: step.selector_quality.clone(),
            text: step.text.clone(),
            url: step.url.clone(),
            key: step.key.clone(),
            value: step.value.clone(),
            delay: step.delay,
            scroll_x: step.scroll_x,
            scroll_y: step.scroll_y,
            scene_scroll_x: step.scene_scroll_x,
            scene_scroll_y: step.scene_scroll_y,
            highlight_selector: None,
            highlights: vec![],
            blur_regions: vec![],
            arrows: vec![],
            hotspots: vec![],
            popups: vec![],
            zoom_regions: vec![],
        }
    }
}

#[cfg(test)]
mod tour_track_tests {
    use super::*;

    /// A bundle with: a setup `wait` (no callout, dropped), a click with a
    /// callout + text anchor, a callout-less click (dropped), and a `type` with
    /// an aria anchor. Guards the projection contract shared by CLI + dashboard.
    fn sample_manifest() -> BundleManifest {
        let json = serde_json::json!({
            "version": 1,
            "viewport": { "width": 1440, "height": 900 },
            "steps": [
                { "file": "steps/0.webp", "action": "wait",
                  "selector": "main", "name": "Wait" },
                { "file": "steps/1.webp", "action": "click",
                  "selector": "a[href$=\"/new\"]", "name": "Open",
                  "targetText": "New project",
                  "highlights": [{ "bounds": {"x":0,"y":0,"width":1,"height":1},
                                   "callout": "Click here" }] },
                { "file": "steps/2.webp", "action": "click",
                  "selector": ".no-callout", "name": "Silent" },
                { "file": "steps/3.webp", "action": "type",
                  "selector": "#name", "name": "Name it",
                  "targetAria": "Project name",
                  "highlights": [{ "bounds": {"x":0,"y":0,"width":1,"height":1},
                                   "callout": "Type a name" }] }
            ]
        });
        serde_json::from_value(json).expect("valid manifest")
    }

    #[test]
    fn projects_only_interactive_steps_with_callouts() {
        let track = sample_manifest().to_tour_track();
        assert_eq!(
            track.steps.len(),
            2,
            "wait + callout-less click are dropped"
        );
        assert_eq!(track.steps[0].selector, "a[href$=\"/new\"]");
        assert_eq!(track.steps[0].advance, TourAdvance::Click);
        assert_eq!(track.steps[1].advance, TourAdvance::Input);
    }

    #[test]
    fn carries_drift_fallback_anchors() {
        let track = sample_manifest().to_tour_track();
        // click step -> text anchor; type step -> aria anchor.
        assert_eq!(
            track.steps[0].fallback.as_ref().unwrap().text.as_deref(),
            Some("New project")
        );
        assert!(track.steps[0].fallback.as_ref().unwrap().aria.is_none());
        assert_eq!(
            track.steps[1].fallback.as_ref().unwrap().aria.as_deref(),
            Some("Project name")
        );
    }

    #[test]
    fn advance_serializes_as_typed_object() {
        // The player expects `{ "type": "click" }` / `{ "type": "input" }`.
        assert_eq!(
            serde_json::to_value(TourAdvance::Click).unwrap(),
            serde_json::json!({ "type": "click" })
        );
        assert_eq!(
            serde_json::to_value(TourAdvance::Input).unwrap(),
            serde_json::json!({ "type": "input" })
        );
    }

    #[test]
    fn empty_fallback_is_omitted() {
        // A step with no target_text/target_aria must not emit a `fallback` key.
        let json = serde_json::json!({
            "version": 1,
            "viewport": { "width": 800, "height": 600 },
            "steps": [
                { "file": "s.webp", "action": "click", "selector": "#go", "name": "Go",
                  "highlights": [{ "bounds": {"x":0,"y":0,"width":1,"height":1},
                                   "callout": "Go" }] }
            ]
        });
        let manifest: BundleManifest = serde_json::from_value(json).unwrap();
        let step_json = serde_json::to_value(&manifest.to_tour_track().steps[0]).unwrap();
        assert!(
            step_json.get("fallback").is_none(),
            "no anchors -> no fallback key"
        );
    }

    #[test]
    fn callout_read_from_first_captioned_highlight() {
        // A decorative highlight (no callout) sits first; the captioned one is
        // second. The step must still project, using the second's callout.
        let json = serde_json::json!({
            "version": 1,
            "viewport": { "width": 800, "height": 600 },
            "steps": [
                { "file": "s.webp", "action": "click", "selector": "#go", "name": "Go",
                  "highlights": [
                    { "bounds": {"x":0,"y":0,"width":1,"height":1} },
                    { "bounds": {"x":0,"y":0,"width":1,"height":1}, "callout": "Click go" }
                  ] }
            ]
        });
        let manifest: BundleManifest = serde_json::from_value(json).unwrap();
        let track = manifest.to_tour_track();
        assert_eq!(
            track.steps.len(),
            1,
            "step with a captioned highlight is kept"
        );
        assert_eq!(track.steps[0].body, "Click go");
    }

    /// One click step whose callout highlight sits at (100,100)-(150,120),
    /// carrying both drift anchors and a caller-supplied blur region.
    fn manifest_with_blur(blur: serde_json::Value) -> BundleManifest {
        let json = serde_json::json!({
            "version": 1,
            "viewport": { "width": 800, "height": 600 },
            "steps": [
                { "file": "s.webp", "action": "click", "selector": "#go", "name": "Go",
                  "targetText": "Secret name", "targetAria": "Secret field",
                  "highlights": [{ "bounds": {"x":100,"y":100,"width":50,"height":20},
                                   "callout": "Click go" }],
                  "blur_regions": blur }
            ]
        });
        serde_json::from_value(json).expect("valid manifest")
    }

    #[test]
    fn blur_over_callout_highlight_omits_fallback() {
        // Blur sits squarely inside the callout highlight -> record-time text of
        // that element must not resurface as a plaintext fallback.
        let track = manifest_with_blur(
            serde_json::json!([{ "x": 110, "y": 105, "width": 30, "height": 10 }]),
        )
        .to_tour_track();
        assert_eq!(track.steps.len(), 1);
        assert!(
            track.steps[0].fallback.is_none(),
            "blur covering the callout highlight redacts the fallback anchors"
        );
    }

    #[test]
    fn blur_elsewhere_keeps_fallback() {
        // Blur is far from the callout highlight -> anchors are preserved.
        let track =
            manifest_with_blur(serde_json::json!([{ "x": 0, "y": 0, "width": 10, "height": 10 }]))
                .to_tour_track();
        assert_eq!(track.steps.len(), 1);
        let fallback = track.steps[0]
            .fallback
            .as_ref()
            .expect("non-overlapping blur leaves fallback intact");
        assert_eq!(fallback.text.as_deref(), Some("Secret name"));
        assert_eq!(fallback.aria.as_deref(), Some("Secret field"));
    }

    #[test]
    fn blur_over_callout_without_anchors_is_unchanged() {
        // A blur intersects the callout highlight, but the step never carried
        // drift anchors — projection is unaffected (no fallback either way).
        let json = serde_json::json!({
            "version": 1,
            "viewport": { "width": 800, "height": 600 },
            "steps": [
                { "file": "s.webp", "action": "click", "selector": "#go", "name": "Go",
                  "highlights": [{ "bounds": {"x":100,"y":100,"width":50,"height":20},
                                   "callout": "Click go" }],
                  "blur_regions": [{ "x": 110, "y": 105, "width": 30, "height": 10 }] }
            ]
        });
        let manifest: BundleManifest = serde_json::from_value(json).unwrap();
        let track = manifest.to_tour_track();
        assert_eq!(track.steps.len(), 1, "step still projects");
        assert_eq!(track.steps[0].body, "Click go");
        assert!(track.steps[0].fallback.is_none());
    }

    #[test]
    fn blurred_fallback_absent_from_serialized_json() {
        // The redacted fallback must not appear as a `fallback` key on the wire.
        let track = manifest_with_blur(
            serde_json::json!([{ "x": 110, "y": 105, "width": 30, "height": 10 }]),
        )
        .to_tour_track();
        let step_json = serde_json::to_value(&track.steps[0]).unwrap();
        assert!(
            step_json.get("fallback").is_none(),
            "blurred anchors -> no fallback key in JSON"
        );
    }
}
