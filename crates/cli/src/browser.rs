use std::sync::Arc;

use chromiumoxide::Page;
use chromiumoxide::browser::{Browser as CdpBrowser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat;
use futures::StreamExt;
use manifest::{ElementBounds, Point2D, Viewport};

use crate::error::CliError;

/// Wrapper around a CDP browser instance.
pub struct Browser {
    _browser: Arc<CdpBrowser>,
    _handle: tokio::task::JoinHandle<()>,
    page: Arc<Page>,
}

impl Browser {
    /// Launch a Chrome/Chromium instance via CDP.
    pub async fn launch(viewport: &Viewport, headless: bool) -> Result<Self, CliError> {
        let device_scale_factor = viewport.device_scale_factor.unwrap_or(1.0);
        let mut builder = BrowserConfig::builder()
            .window_size(viewport.width, viewport.height)
            .viewport(chromiumoxide::handler::viewport::Viewport {
                width: viewport.width,
                height: viewport.height,
                device_scale_factor: Some(device_scale_factor),
                emulating_mobile: false,
                is_landscape: false,
                has_touch: false,
            });

        if headless {
            builder = builder.arg("--headless=new");
        }

        if let Ok(chrome_path) = std::env::var("CHROME_PATH") {
            builder = builder.chrome_executable(chrome_path);
        }

        builder = builder
            .arg("--disable-background-networking")
            .arg("--disable-default-apps")
            .arg("--no-first-run");

        // CI environments (GitHub Actions, etc.) need sandbox disabled
        if std::env::var("CI").is_ok() {
            builder = builder
                .no_sandbox()
                .arg("--disable-gpu")
                .arg("--disable-dev-shm-usage");
        }

        let config = builder
            .build()
            .map_err(|e| CliError::Browser(format!("Failed to build browser config: {e}")))?;

        let (browser, mut handler) = CdpBrowser::launch(config)
            .await
            .map_err(|e| CliError::Browser(format!("Failed to launch browser: {e}")))?;

        let handle = tokio::spawn(async move {
            while let Some(event) = handler.next().await {
                let _ = event;
            }
        });

        let page = browser
            .new_page("about:blank")
            .await
            .map_err(|e| CliError::Browser(format!("Failed to open page: {e}")))?;

        Ok(Self {
            _browser: Arc::new(browser),
            _handle: handle,
            page: Arc::new(page),
        })
    }

    /// Set the preferred color scheme via CDP Emulation.
    pub async fn set_color_scheme(&self, scheme: &str) -> Result<(), CliError> {
        use chromiumoxide::cdp::browser_protocol::emulation::{
            MediaFeature, SetEmulatedMediaParams,
        };
        let feature = MediaFeature::new("prefers-color-scheme", scheme);
        let params = SetEmulatedMediaParams::builder()
            .features(vec![feature])
            .build();
        self.page
            .execute(params)
            .await
            .map_err(|e| CliError::Browser(format!("Failed to set color scheme: {e}")))?;
        Ok(())
    }

    /// Navigate to a URL and wait for the page to load.
    pub async fn navigate(&self, url: &str) -> Result<(), CliError> {
        self.page
            .goto(url)
            .await
            .map_err(|e| CliError::Browser(format!("Navigation failed: {e}")))?;
        self.page
            .wait_for_navigation()
            .await
            .map_err(|e| CliError::Browser(format!("Wait for navigation failed: {e}")))?;
        Ok(())
    }

    /// Wait for the network to be idle (simple heuristic: sleep).
    pub async fn wait_idle(&self, timeout_ms: u64) {
        tokio::time::sleep(tokio::time::Duration::from_millis(timeout_ms)).await;
    }

    /// Capture a viewport-only screenshot as WebP bytes.
    pub async fn screenshot(&self) -> Result<Vec<u8>, CliError> {
        let bytes = self
            .page
            .screenshot(
                chromiumoxide::page::ScreenshotParams::builder()
                    .format(CaptureScreenshotFormat::Webp)
                    .quality(85)
                    .build(),
            )
            .await
            .map_err(|e| CliError::Browser(format!("Screenshot failed: {e}")))?;
        Ok(bytes)
    }

    /// Capture a viewport-only screenshot as WebP bytes with configurable quality.
    pub async fn screenshot_jpeg(&self, quality: u8) -> Result<Vec<u8>, CliError> {
        let bytes = self
            .page
            .screenshot(
                chromiumoxide::page::ScreenshotParams::builder()
                    .format(CaptureScreenshotFormat::Webp)
                    .quality(quality as i64)
                    .build(),
            )
            .await
            .map_err(|e| CliError::Browser(format!("Screenshot (JPEG) failed: {e}")))?;
        Ok(bytes)
    }

    /// Get the bounding rectangle of an element by CSS selector.
    pub async fn get_bounds(&self, selector: &str) -> Result<Option<ElementBounds>, CliError> {
        let js = format!(
            r#"
            (() => {{
                const el = document.querySelector({selector});
                if (!el) return null;
                const r = el.getBoundingClientRect();
                return {{ x: r.x, y: r.y, width: r.width, height: r.height }};
            }})()
            "#,
            selector = serde_json::to_string(selector)?
        );
        let result = self
            .page
            .evaluate(js)
            .await
            .map_err(|e| CliError::Browser(format!("getBoundingClientRect failed: {e}")))?;

        let value = result.into_value::<serde_json::Value>().ok();
        match value {
            Some(serde_json::Value::Object(obj)) => {
                let bounds = ElementBounds {
                    x: obj.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0),
                    y: obj.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0),
                    width: obj.get("width").and_then(|v| v.as_f64()).unwrap_or(0.0),
                    height: obj.get("height").and_then(|v| v.as_f64()).unwrap_or(0.0),
                };
                Ok(Some(bounds))
            }
            _ => Ok(None),
        }
    }

    /// Get the center point of an element by CSS selector.
    pub async fn get_element_center(&self, selector: &str) -> Result<Option<Point2D>, CliError> {
        let js = format!(
            r#"
            (() => {{
                const el = document.querySelector({selector});
                if (!el) return null;
                const r = el.getBoundingClientRect();
                return {{ x: r.x + r.width / 2, y: r.y + r.height / 2 }};
            }})()
            "#,
            selector = serde_json::to_string(selector)?
        );
        let result = self
            .page
            .evaluate(js)
            .await
            .map_err(|e| CliError::Browser(format!("Failed to get element center: {e}")))?;

        let value = result.into_value::<serde_json::Value>().ok();
        match value {
            Some(serde_json::Value::Object(obj)) => {
                let point = Point2D {
                    x: obj.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0),
                    y: obj.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0),
                };
                Ok(Some(point))
            }
            _ => Ok(None),
        }
    }

    pub async fn set_scroll_position(&self, x: f64, y: f64) -> Result<(), CliError> {
        let js = format!("window.scrollTo({{ left: {x}, top: {y}, behavior: 'instant' }});");
        self.page
            .evaluate(js)
            .await
            .map_err(|e| CliError::Browser(format!("Failed to set scroll position: {e}")))?;
        Ok(())
    }

    pub async fn click_at_point(&self, x: f64, y: f64) -> Result<(), CliError> {
        let js = format!(
            r#"
            (() => {{
                const x = {x};
                const y = {y};
                const el = document.elementFromPoint(x, y);
                if (!el) return false;
                const opts = {{ bubbles: true, cancelable: true, clientX: x, clientY: y }};
                el.dispatchEvent(new MouseEvent('mouseover', opts));
                el.dispatchEvent(new MouseEvent('mousedown', opts));
                el.dispatchEvent(new MouseEvent('mouseup', opts));
                el.dispatchEvent(new MouseEvent('click', opts));
                return true;
            }})()
            "#
        );
        let result = self
            .page
            .evaluate(js)
            .await
            .map_err(|e| CliError::Browser(format!("Point click failed: {e}")))?;
        let clicked = result.into_value::<bool>().unwrap_or(false);
        if clicked {
            Ok(())
        } else {
            Err(CliError::Action(format!(
                "Could not click at viewport point ({x:.1}, {y:.1})"
            )))
        }
    }

    /// Get a reference to the underlying CDP page.
    pub fn page(&self) -> &Page {
        &self.page
    }
}
