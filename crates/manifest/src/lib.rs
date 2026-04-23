use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ============================================================================
// Recording config types (stepshots.config.json — shared across CLI, extension, etc.)
// ============================================================================

/// Top-level config from `stepshots.config.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepshotsConfig {
    pub base_url: String,
    #[serde(default = "default_viewport")]
    pub viewport: Viewport,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<DemoFormat>,
    #[serde(default = "default_delay")]
    pub default_delay: u64,
    /// Color scheme for recording: "dark" or "light". Defaults to browser default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    pub tutorials: HashMap<String, TutorialConfig>,
}

/// A single tutorial within the config.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TutorialConfig {
    pub url: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub steps: Vec<StepConfig>,
}

/// A single step action in a tutorial.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepConfig {
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selector_quality: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scroll_x: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scroll_y: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene_scroll_x: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene_scroll_y: Option<f64>,
    /// Optional CSS selector used only for resolving highlight bounds.
    /// When set, overrides `selector` for highlight resolution so that
    /// action steps (scroll, navigate) can target one element while
    /// highlighting another.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highlight_selector: Option<String>,
    #[serde(alias = "annotations")]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub highlights: Vec<HighlightConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blur_regions: Vec<BlurConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arrows: Vec<ArrowConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hotspots: Vec<HotspotConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub popups: Vec<PopupConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub zoom_regions: Vec<ZoomConfig>,
}

/// Highlight config for a step in the recording config.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HighlightConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<ElementBounds>,
    #[serde(alias = "highlight")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_border: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arrow: Option<bool>,
}

/// Blur region config — resolved from CSS selector to element bounds.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlurConfig {
    pub selector: String,
}

/// Arrow config — resolved from two CSS selectors to element center points.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArrowConfig {
    pub from_selector: String,
    pub to_selector: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curvature: Option<f64>,
}

/// Hotspot config — resolved from CSS selector to element center point.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HotspotConfig {
    pub selector: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_click_target: Option<bool>,
}

/// Popup config — resolved from CSS selector to element center point.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PopupConfig {
    pub selector: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
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
#[serde(rename_all = "camelCase")]
pub struct ZoomConfig {
    pub selector: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub magnification: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<u32>,
}

/// Preset format for demo recording viewport dimensions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
pub struct ElementBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub z_index: Option<u32>,
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
#[serde(rename_all = "camelCase")]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
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
