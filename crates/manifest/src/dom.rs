//! DOM structural extract — input to the sandbox generator.
//!
//! This is **not** a replay format. Nothing here is rendered directly; it is a
//! compact structural map of a page that an AI generator uses as ground truth
//! when rebuilding an interactive sandbox. Deliberately *not* captured: asset
//! bytes, stylesheets, scripts, or anything that would let this be replayed as
//! a page snapshot. See `docs/dom-extract-spec.md`.
//!
//! Two invariants this format exists to uphold:
//!
//! 1. **Size.** A whole-page snapshot is 1–5 MB. An extract targets ≤150 KB raw
//!    per step by pruning invisible nodes, recording only a computed-style
//!    *delta* against the parent, and referencing assets instead of inlining
//!    them.
//! 2. **Redaction happens at capture.** Unlike blur — which rides in the
//!    manifest as metadata and is baked into public PNGs server-side — an
//!    extract carries real text. It must already be redacted when it is written
//!    to the bundle. There is no publish-time redaction pass for extracts.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::Viewport;

/// Current extract format version, written as `v`.
pub const DOM_EXTRACT_VERSION: u32 = 1;

/// A single step's structural extract, stored as `dom/{i}.json` in the bundle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomExtract {
    /// Format version. See [`DOM_EXTRACT_VERSION`].
    pub v: u32,
    /// Viewport the page was measured in. Bounds are meaningless without it.
    pub viewport: Viewport,
    /// Page URL at capture time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// True when a budget (node count or depth) was hit and nodes were dropped.
    #[serde(default, skip_serializing_if = "is_false")]
    pub truncated: bool,
    /// Capture counters. Chiefly useful for asserting redaction actually ran.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stats: Option<DomStats>,
    /// Design tokens inferred from the page — the generator's palette and scale.
    #[serde(default)]
    pub tokens: DomTokens,
    /// Root of the pruned element tree.
    pub root: DomNode,
}

impl DomExtract {
    /// Visit every node depth-first, root included.
    pub fn visit(&self, f: &mut impl FnMut(&DomNode)) {
        self.root.visit(f);
    }

    /// Total nodes retained after pruning.
    pub fn node_count(&self) -> usize {
        let mut n = 0;
        self.visit(&mut |_| n += 1);
        n
    }

    /// Whether any retained node's text contains `needle`, case-insensitively.
    ///
    /// Exists so redaction can be asserted rather than assumed — a test can
    /// record a page with known PII and require this to be `false`.
    pub fn contains_text(&self, needle: &str) -> bool {
        let needle = needle.to_lowercase();
        let mut found = false;
        self.visit(&mut |node| {
            if node
                .txt
                .as_ref()
                .is_some_and(|t| t.to_lowercase().contains(&needle))
            {
                found = true;
            }
        });
        found
    }
}

/// Counters describing what the walker produced.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomStats {
    /// Nodes retained after pruning.
    pub nodes: u32,
    /// Nodes whose text was redacted (blur overlap, redact selector, or scrub).
    pub redacted: u32,
}

/// Design tokens sampled from the page: the most frequent values, most common
/// first. Gives the generator a design system instead of making it infer one
/// from pixels — the specific failure the screenshot-only spike exhibited.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomTokens {
    /// Colors as CSS hex or `rgb()`, most frequent first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub colors: Vec<String>,
    /// Font sizes in CSS pixels, most frequent first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub font_sizes: Vec<f64>,
    /// Font family stack of the document body, in declaration order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub font_family: Vec<String>,
    /// Border radii in CSS pixels, most frequent first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub radii: Vec<f64>,
    /// Padding/gap values in CSS pixels, most frequent first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spacing: Vec<f64>,
}

/// What a node is, when it is not plain markup the generator can rebuild.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DomNodeKind {
    /// `<canvas>` or a chart-shaped `<svg>`. The generator synthesizes a real
    /// chart component with fake series; only bounds and palette are ground truth.
    Chart,
    /// Cross-origin iframe, shadow host, or WebGL surface. No text is extracted;
    /// the generator emits a correctly-sized placeholder.
    Opaque,
    /// `<video>` / `<audio>`.
    Media,
}

/// One retained element.
///
/// Field names are short on purpose — this struct is repeated up to 1,500 times
/// per step and key names dominate the serialized size.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomNode {
    /// Lowercase tag name.
    pub tag: String,
    /// Raw `class` attribute, verbatim.
    ///
    /// Load-bearing: on a utility-CSS app (Tailwind/DaisyUI) this *is* the type
    /// scale, spacing scale and responsive behaviour. On BEM/CSS-modules apps it
    /// is opaque and [`DomNode::s`] carries the weight instead — which is why
    /// both are captured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cls: Option<String>,
    /// Bounds in viewport CSS pixels: `[x, y, width, height]`.
    pub b: [f64; 4],
    /// Computed styles that differ from the parent, from a fixed whitelist.
    /// Inherited and default values are omitted. Sorted for stable output.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub s: BTreeMap<String, String>,
    /// The node's own text, trimmed and whitespace-collapsed. Excludes text
    /// belonging to child elements.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txt: Option<String>,
    /// `role` attribute, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// `aria-label`, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aria: Option<String>,
    /// Non-markup classification. Absent for ordinary elements.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<DomNodeKind>,
    /// Image reference. Never contains bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<AssetRef>,
    /// Colors sampled from a chart or opaque surface, so a placeholder can be
    /// tinted plausibly.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub palette: Vec<String>,
    /// Set when this node's text was removed by redaction. The node is retained
    /// so layout survives; only content is gone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redacted: Option<bool>,
    /// Retained children, in document order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kids: Vec<DomNode>,
}

impl DomNode {
    /// Visit this node and its descendants depth-first.
    pub fn visit(&self, f: &mut impl FnMut(&DomNode)) {
        f(self);
        for kid in &self.kids {
            kid.visit(f);
        }
    }
}

/// A reference to an image. Bytes are never captured: the generator emits a
/// placeholder at the right size and colour, which is what keeps an extract in
/// kilobytes rather than megabytes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetRef {
    /// Source URL as authored. Dropped entirely when the node is redacted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub src: Option<String>,
    /// `alt` text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alt: Option<String>,
    /// Intrinsic width in pixels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nw: Option<f64>,
    /// Intrinsic height in pixels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nh: Option<f64>,
    /// Dominant colour, sampled from the step screenshot at this node's bounds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(tag: &str, txt: Option<&str>, kids: Vec<DomNode>) -> DomNode {
        DomNode {
            tag: tag.to_string(),
            cls: None,
            b: [0.0, 0.0, 10.0, 10.0],
            s: BTreeMap::new(),
            txt: txt.map(str::to_string),
            role: None,
            aria: None,
            kind: None,
            asset: None,
            palette: Vec::new(),
            redacted: None,
            kids,
        }
    }

    fn extract(root: DomNode) -> DomExtract {
        DomExtract {
            v: DOM_EXTRACT_VERSION,
            viewport: Viewport {
                width: 1440,
                height: 810,
                device_scale_factor: None,
            },
            url: None,
            truncated: false,
            stats: None,
            tokens: DomTokens::default(),
            root,
        }
    }

    #[test]
    fn counts_and_visits_every_node() {
        let e = extract(node(
            "div",
            None,
            vec![
                node("h1", Some("Hi"), vec![]),
                node("p", Some("There"), vec![]),
            ],
        ));
        assert_eq!(e.node_count(), 3);
    }

    #[test]
    fn contains_text_finds_nested_matches_case_insensitively() {
        let e = extract(node(
            "div",
            None,
            vec![node("td", Some("alex@northwind.io"), vec![])],
        ));
        assert!(e.contains_text("ALEX@NORTHWIND.IO"));
        assert!(!e.contains_text("nothere@example.com"));
    }

    #[test]
    fn omits_empty_fields_from_json() {
        let json = serde_json::to_string(&extract(node("div", None, vec![]))).unwrap();
        // Empty style maps, absent text and default tokens must not cost bytes.
        assert!(!json.contains("\"s\""), "{json}");
        assert!(!json.contains("\"txt\""), "{json}");
        assert!(!json.contains("\"kids\""), "{json}");
        assert!(!json.contains("\"truncated\""), "{json}");
    }

    #[test]
    fn round_trips_through_json() {
        let mut n = node("canvas", None, vec![]);
        n.kind = Some(DomNodeKind::Chart);
        n.palette = vec!["#3b82f6".into()];
        let before = extract(n);
        let json = serde_json::to_string(&before).unwrap();
        let after: DomExtract = serde_json::from_str(&json).unwrap();
        assert_eq!(before, after);
    }
}
