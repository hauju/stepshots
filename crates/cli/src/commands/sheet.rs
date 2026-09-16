//! `stepshots sheet` — one image summarising a whole recording.
//!
//! A `.stepshot` bundle is a zip of screenshots: to see what a recording
//! actually captured you either upload it or unzip it. Neither fits a pull
//! request, a bug report, or a glance before publishing. This lays every step
//! out on one page and photographs it.
//!
//! Rendered through the browser the CLI already drives rather than a pixel
//! compositor: the bundle's own step images go straight in as data URIs, so
//! there is no image codec to add and the labels are just HTML.

use std::path::{Path, PathBuf};

use base64::Engine as _;
use manifest::{BundleManifest, Viewport};

use crate::browser::Browser;
use crate::bundler::read_bundle;
use crate::error::CliError;

/// Sheet width in CSS pixels. Wide enough for three desktop shots to stay
/// readable, narrow enough to open without scrolling sideways.
const SHEET_WIDTH: u32 = 1440;
const COLUMNS: usize = 3;

pub async fn run(bundle: &Path, output: Option<PathBuf>) -> Result<(), CliError> {
    let (manifest, screenshots, _, _) = read_bundle(bundle)?;
    if screenshots.is_empty() {
        return Err(CliError::Bundle(format!(
            "{}: bundle has no steps",
            bundle.display()
        )));
    }

    let out = output.unwrap_or_else(|| bundle.with_extension("sheet.png"));
    let html = render_html(&manifest, &screenshots, bundle);

    // Served from disk rather than through a local HTTP server: a `file://`
    // page loads its own data URIs without one, and nothing here needs an
    // origin. The name carries a random component because the file inlines
    // every step of the demo — a predictable path in a shared /tmp is both a
    // disclosure and a symlink target.
    let html_path = std::env::temp_dir().join(format!(
        "stepshots-sheet-{}-{:016x}.html",
        std::process::id(),
        rand::random::<u64>()
    ));
    std::fs::write(&html_path, &html)?;

    // Every exit from here has to delete it, so the capture is a separate call
    // and the `?` waits until after cleanup.
    let png = capture(&html_path).await;
    let _ = std::fs::remove_file(&html_path);
    let png = png?;

    std::fs::write(&out, &png)?;
    println!(
        "Wrote {} ({} steps, {:.0} KB)",
        out.display(),
        screenshots.len(),
        png.len() as f64 / 1024.0
    );
    Ok(())
}

/// Render the page at `html_path` and photograph the whole of it.
async fn capture(html_path: &Path) -> Result<Vec<u8>, CliError> {
    let viewport = Viewport {
        width: SHEET_WIDTH,
        // Deliberately tiny: a full-page capture is never *shorter* than the
        // viewport, so anything taller than the grid becomes dead space below
        // the last row.
        height: 200,
        device_scale_factor: None,
    };
    let browser = Browser::launch(&viewport, true, None).await?;
    browser.navigate(&file_url(html_path)).await?;
    browser.wait_idle(300).await;
    browser.screenshot_full_png().await
}

/// `file://` URL for a local path.
///
/// Encoded, not interpolated: a `#` in `TMPDIR` would otherwise truncate the
/// URL into a fragment, Chrome would load nothing, and the capture would
/// succeed against a blank page — a sheet that is wrong without failing.
fn file_url(path: &Path) -> String {
    let mut out = String::from("file://");
    for b in path.to_string_lossy().bytes() {
        match b {
            b'/' | b'-' | b'_' | b'.' | b'~' | b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Escape for HTML text content and quoted attributes.
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Build the sheet page. Step images are WebP in the bundle; the browser
/// decodes them, so they go in verbatim.
fn render_html(manifest: &BundleManifest, screenshots: &[Vec<u8>], bundle: &Path) -> String {
    let b64 = base64::engine::general_purpose::STANDARD;
    let title = bundle.file_stem().unwrap_or_default().to_string_lossy();
    // Cells keep the recorded aspect ratio, so a mobile demo reads as a column
    // of phones rather than three letterboxed slivers.
    let ratio = format!("{}/{}", manifest.viewport.width, manifest.viewport.height);

    let cells: String = screenshots
        .iter()
        .enumerate()
        .map(|(i, png)| {
            let step = manifest.steps.get(i);
            // Prefer the author's name; fall back to what the step did, which
            // is the only label an unnamed step has.
            let label = step
                .and_then(|s| s.name.clone())
                .or_else(|| {
                    step.and_then(|s| {
                        let action = s.action.clone()?;
                        Some(match s.selector.as_deref() {
                            Some(sel) => format!("{action} {sel}"),
                            None => action,
                        })
                    })
                })
                .unwrap_or_default();
            format!(
                r#"<figure><div class="shot"><img src="data:image/webp;base64,{data}" alt="Step {n}"></div><figcaption><span class="n">{n}</span>{label}</figcaption></figure>"#,
                data = b64.encode(png),
                n = i + 1,
                label = esc(&label),
            )
        })
        .collect();

    format!(
        r#"<!doctype html><html><head><meta charset="utf-8"><style>
*{{box-sizing:border-box}}
body{{margin:0;padding:32px;width:{width}px;background:#0b0d12;color:#e8eaf0;
  font:14px/1.4 ui-sans-serif,system-ui,-apple-system,"Segoe UI",sans-serif}}
h1{{margin:0 0 4px;font-size:20px;font-weight:600}}
.meta{{margin:0 0 24px;color:#8b93a7;font-size:13px}}
.grid{{display:grid;grid-template-columns:repeat({cols},1fr);gap:20px}}
figure{{margin:0}}
.shot{{background:#161a23;border:1px solid #262c39;border-radius:8px;overflow:hidden;
  aspect-ratio:{ratio};display:flex;align-items:flex-start;justify-content:center}}
.shot img{{width:100%;height:100%;object-fit:contain;object-position:top center;display:block}}
figcaption{{display:flex;gap:8px;align-items:baseline;margin-top:8px;color:#b8bfd0;font-size:13px;
  white-space:nowrap;overflow:hidden;text-overflow:ellipsis}}
.n{{flex:none;min-width:22px;height:22px;padding:0 6px;border-radius:11px;background:#2b334a;color:#cfd6e6;font-size:12px;font-weight:600;text-align:center;line-height:22px}}
</style></head><body>
<h1>{title}</h1>
<p class="meta">{count} steps · {file}</p>
<div class="grid">{cells}</div>
</body></html>"#,
        width = SHEET_WIDTH,
        cols = COLUMNS,
        ratio = ratio,
        title = esc(&title),
        count = screenshots.len(),
        file = esc(&bundle.file_name().unwrap_or_default().to_string_lossy()),
        cells = cells,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(width: u32, height: u32, steps: serde_json::Value) -> BundleManifest {
        serde_json::from_value(serde_json::json!({
            "version": 1,
            "viewport": { "width": width, "height": height },
            "steps": steps,
        }))
        .unwrap()
    }

    #[test]
    fn cells_keep_the_recorded_aspect_ratio() {
        let m = manifest(390, 844, serde_json::json!([{ "file": "steps/0.webp" }]));
        let html = render_html(&m, &[vec![0u8]], Path::new("demo.stepshot"));
        assert!(html.contains("aspect-ratio:390/844"), "{html}");
    }

    /// An unnamed step still needs a caption, or the sheet is a wall of
    /// unlabelled screenshots — which is the thing it exists to replace.
    #[test]
    fn unnamed_steps_fall_back_to_what_they_did() {
        let m = manifest(
            1280,
            800,
            serde_json::json!([
                { "file": "steps/0.webp", "name": "Open settings" },
                { "file": "steps/1.webp", "action": "click", "selector": "#save" },
            ]),
        );
        let html = render_html(&m, &[vec![0u8], vec![0u8]], Path::new("demo.stepshot"));
        assert!(html.contains("Open settings"));
        assert!(html.contains("click #save"));
    }

    /// A `#` in the path would truncate the URL into a fragment, and the
    /// capture would quietly photograph a blank error page.
    #[test]
    fn file_urls_are_encoded() {
        assert_eq!(
            file_url(Path::new("/tmp/build #17/sheet.html")),
            "file:///tmp/build%20%2317/sheet.html"
        );
        assert_eq!(
            file_url(Path::new("/var/folders/x/sheet.html")),
            "file:///var/folders/x/sheet.html"
        );
    }

    /// Step names and selectors are author input and land in the page verbatim.
    #[test]
    fn labels_are_escaped() {
        let m = manifest(
            1280,
            800,
            serde_json::json!([{ "file": "steps/0.webp", "name": "<img src=x onerror=alert(1)>" }]),
        );
        let html = render_html(&m, &[vec![0u8]], Path::new("demo.stepshot"));
        assert!(!html.contains("<img src=x"), "{html}");
        assert!(html.contains("&lt;img src=x onerror=alert(1)&gt;"));
    }
}
