use std::sync::Arc;

use chromiumoxide::Page;
use chromiumoxide::browser::{Browser as CdpBrowser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::network::{
    CookieParam, CookieSameSite, SetCookiesParams, TimeSinceEpoch,
};
use chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat;
use chromiumoxide::cdp::browser_protocol::target::TargetId;
use futures::StreamExt;
use manifest::{ElementBounds, Point2D, Viewport};

use crate::error::CliError;

/// Where a run's browser session comes from.
///
/// The two are alternatives for the same job — arriving at a page already
/// logged in — and travel together through every command that records or
/// replays, so they are passed as one value rather than two parallel options.
/// `profile_dir` is the local answer; `storage_state` is the one that works in
/// CI. Both absent means an anonymous session, which is the common case.
#[derive(Default, Clone, Copy)]
pub struct SessionSource<'a> {
    pub profile_dir: Option<&'a std::path::Path>,
    pub storage_state: Option<&'a crate::storage_state::StorageState>,
}

/// The session flags exactly as they arrive from the command line, before any
/// file has been read. [`SessionArgs::load`] turns this into a
/// [`SessionSource`].
#[derive(Default, Clone, Copy)]
pub struct SessionArgs<'a> {
    pub profile_dir: Option<&'a std::path::Path>,
    pub storage_state: Option<&'a std::path::Path>,
}

impl<'a> SessionArgs<'a> {
    /// Read the storage-state file, if one was given.
    ///
    /// Callers do this once up front, before launching anything: a malformed or
    /// empty session file should fail immediately, not after the first tutorial
    /// has quietly recorded a set of logged-out screenshots.
    pub fn load(
        &self,
        announce: bool,
    ) -> Result<Option<crate::storage_state::StorageState>, CliError> {
        let Some(path) = self.storage_state else {
            return Ok(None);
        };
        let state = crate::storage_state::StorageState::load(path)?;
        if announce {
            println!(
                "Using session state: {} ({})",
                path.display(),
                state.summary()
            );
        }
        Ok(Some(state))
    }

    /// Pair the flags with an already-loaded state.
    pub fn with_state(
        &self,
        state: Option<&'a crate::storage_state::StorageState>,
    ) -> SessionSource<'a> {
        SessionSource {
            profile_dir: self.profile_dir,
            storage_state: state,
        }
    }
}

/// Wrapper around a CDP browser instance.
pub struct Browser {
    browser: Arc<CdpBrowser>,
    _handle: tokio::task::JoinHandle<()>,
    // Swappable so the recorder can follow links that open a new tab
    // (target="_blank") instead of staying on the abandoned page.
    page: std::sync::Mutex<Arc<Page>>,
}

impl Browser {
    /// Launch a Chrome/Chromium instance via CDP.
    ///
    /// `profile_dir` points Chrome at a persistent user-data directory so
    /// logged-in sessions (cookies, local storage) survive across runs —
    /// required for recording authenticated flows.
    pub async fn launch(
        viewport: &Viewport,
        headless: bool,
        profile_dir: Option<&std::path::Path>,
    ) -> Result<Self, CliError> {
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

        // chromiumoxide defaults to headless; headed mode must be requested
        // explicitly or the browser window never appears.
        if headless {
            builder = builder.new_headless_mode();
        } else {
            builder = builder.with_head();
        }

        if let Some(dir) = profile_dir {
            builder = builder.user_data_dir(dir);
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

        let (browser, mut handler) = CdpBrowser::launch(config).await.map_err(|e| {
            CliError::Browser(format!(
                "Failed to launch browser: {e}. Install Google Chrome or Chromium, \
                 or set CHROME_PATH to the executable. Run `stepshots doctor` to check your setup."
            ))
        })?;

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
            browser: Arc::new(browser),
            _handle: handle,
            page: std::sync::Mutex::new(Arc::new(page)),
        })
    }

    /// Target ids of all currently open pages. Snapshot these before an
    /// action to detect tabs the action opens.
    pub async fn page_ids(&self) -> Result<Vec<TargetId>, CliError> {
        let pages = self
            .browser
            .pages()
            .await
            .map_err(|e| CliError::Browser(format!("Failed to list pages: {e}")))?;
        Ok(pages.iter().map(|p| p.target_id().clone()).collect())
    }

    /// If the last action spawned a new tab (e.g. a target="_blank" link),
    /// switch the active page to it so subsequent steps and screenshots
    /// follow the user flow. `seen` is the page-id snapshot taken before the
    /// action; only genuinely new tabs are adopted (restored session tabs or
    /// the New Tab Page are ignored). Returns true if the active page changed.
    pub async fn adopt_new_page(&self, seen: &[TargetId]) -> Result<bool, CliError> {
        let pages = self
            .browser
            .pages()
            .await
            .map_err(|e| CliError::Browser(format!("Failed to list pages: {e}")))?;
        let current_id = self.page().target_id().clone();
        for page in pages {
            let tid = page.target_id().clone();
            if tid == current_id || seen.contains(&tid) {
                continue;
            }
            if page.url().await.ok().flatten().as_deref() == Some("about:blank") {
                continue;
            }
            page.wait_for_navigation().await.ok();
            page.bring_to_front().await.ok();
            *self.page.lock().unwrap() = Arc::new(page);
            return Ok(true);
        }
        Ok(false)
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
        self.page()
            .execute(params)
            .await
            .map_err(|e| CliError::Browser(format!("Failed to set color scheme: {e}")))?;
        Ok(())
    }

    /// Navigate to a URL and wait for the page to load.
    pub async fn navigate(&self, url: &str) -> Result<(), CliError> {
        self.page()
            .goto(url)
            .await
            .map_err(|e| CliError::Browser(format!("Navigation failed: {e}")))?;
        self.page()
            .wait_for_navigation()
            .await
            .map_err(|e| CliError::Browser(format!("Wait for navigation failed: {e}")))?;
        Ok(())
    }

    /// Launch, then seed whatever session the run was given — so callers can
    /// never accidentally navigate before applying it.
    pub async fn launch_with_session(
        viewport: &Viewport,
        headless: bool,
        session: SessionSource<'_>,
    ) -> Result<Self, CliError> {
        let browser = Self::launch(viewport, headless, session.profile_dir).await?;
        if let Some(state) = session.storage_state {
            browser.apply_storage_state(state).await?;
        }
        Ok(browser)
    }

    /// Seed the browser with a saved session so the first navigation lands
    /// already authenticated.
    ///
    /// Must be called before navigating anywhere. Cookies go in over CDP, which
    /// needs no page to be loaded. `localStorage` cannot be written for an
    /// origin the browser is not on, so instead of navigating to each origin in
    /// turn this registers an init script that runs before page scripts on every
    /// document and restores the entries for whichever origin matches.
    pub async fn apply_storage_state(
        &self,
        state: &crate::storage_state::StorageState,
    ) -> Result<(), CliError> {
        let page = self.page();

        let cookies: Vec<CookieParam> = state.cookies.iter().map(to_cookie_param).collect();
        if !cookies.is_empty() {
            // The raw CDP command, not chromiumoxide's `set_cookies` helper.
            // That helper resolves each cookie against the *page's* URL and
            // rejects anything non-http — so on the `about:blank` we are still
            // sitting on it fails with "Blank page can not have cookie". Seeding
            // before the first navigation is the entire point here, and
            // Network.setCookies is happy with a `domain` and no page loaded.
            page.execute(SetCookiesParams::new(cookies))
                .await
                .map_err(|e| {
                    CliError::Browser(format!(
                        "Failed to apply cookies from the storage state: {e}. \
                     Each cookie needs either a `domain` or a `url`."
                    ))
                })?;
        }

        if let Some(script) = state.local_storage_script() {
            page.evaluate_on_new_document(script).await.map_err(|e| {
                CliError::Browser(format!(
                    "Failed to install the localStorage restore script: {e}"
                ))
            })?;
        }

        Ok(())
    }

    /// Wait for the network to be idle (simple heuristic: sleep).
    pub async fn wait_idle(&self, timeout_ms: u64) {
        tokio::time::sleep(tokio::time::Duration::from_millis(timeout_ms)).await;
    }

    /// Capture a viewport-only screenshot as WebP bytes.
    pub async fn screenshot(&self) -> Result<Vec<u8>, CliError> {
        let bytes = self
            .page()
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
            .page()
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
            .page()
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
                    z_index: None,
                };
                Ok(Some(bounds))
            }
            _ => Ok(None),
        }
    }

    /// Whether the element is a password input. Used to redact typed text from
    /// the bundle manifest. Returns `false` when the element isn't found or the
    /// check fails — callers treat this as advisory, not a guarantee.
    pub async fn is_password_input(&self, selector: &str) -> bool {
        let js = format!(
            r#"
            (() => {{
                const el = document.querySelector({selector});
                return !!el && el.type === "password";
            }})()
            "#,
            selector = match serde_json::to_string(selector) {
                Ok(s) => s,
                Err(_) => return false,
            }
        );
        match self.page().evaluate(js).await {
            Ok(result) => result.into_value::<bool>().unwrap_or(false),
            Err(_) => false,
        }
    }

    /// Capture the target element's text + aria-label, for use as a resilient
    /// fallback anchor when the CSS selector later drifts. Returns `(text, aria)`;
    /// both `None` if the element isn't found.
    pub async fn get_element_identity(
        &self,
        selector: &str,
    ) -> Result<(Option<String>, Option<String>), CliError> {
        let js = format!(
            r#"
            (() => {{
                const el = document.querySelector({selector});
                if (!el) return null;
                const raw = (el.textContent || "").replace(/\s+/g, " ").trim();
                const text = raw ? raw.slice(0, 120) : null;
                const aria = el.getAttribute("aria-label") || null;
                return {{ text, aria }};
            }})()
            "#,
            selector = serde_json::to_string(selector)?
        );
        let result = self
            .page()
            .evaluate(js)
            .await
            .map_err(|e| CliError::Browser(format!("element identity capture failed: {e}")))?;
        let value = result.into_value::<serde_json::Value>().ok();
        match value {
            Some(serde_json::Value::Object(obj)) => {
                let text = obj.get("text").and_then(|v| v.as_str()).map(str::to_string);
                let aria = obj.get("aria").and_then(|v| v.as_str()).map(str::to_string);
                Ok((text, aria))
            }
            _ => Ok((None, None)),
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
            .page()
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
        self.page()
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
            .page()
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

    /// Get a handle to the currently active CDP page.
    pub fn page(&self) -> Arc<Page> {
        self.page.lock().unwrap().clone()
    }
}

/// Translate one saved cookie into the CDP parameter shape.
///
/// Two mismatches between the file format and CDP are handled here: Playwright
/// writes `expires: -1` for a session cookie where CDP wants the field absent,
/// and `sameSite` is a free string in JSON but an enum on the wire. An
/// unrecognised `sameSite` is dropped rather than rejected — Chrome applies its
/// own default, and failing a whole recording over one advisory field would be
/// a poor trade.
fn to_cookie_param(cookie: &crate::storage_state::Cookie) -> CookieParam {
    let mut param = CookieParam::new(cookie.name.clone(), cookie.value.clone());
    param.url = cookie.url.clone();
    param.domain = cookie.domain.clone();
    param.path = cookie.path.clone();
    param.secure = cookie.secure;
    param.http_only = cookie.http_only;
    param.expires = cookie.expires.filter(|e| *e > 0.0).map(TimeSinceEpoch::new);
    param.same_site =
        cookie
            .same_site
            .as_deref()
            .and_then(|s| match s.to_ascii_lowercase().as_str() {
                "strict" => Some(CookieSameSite::Strict),
                "lax" => Some(CookieSameSite::Lax),
                "none" => Some(CookieSameSite::None),
                _ => None,
            });
    param
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage_state::Cookie;

    fn cookie(json: &str) -> Cookie {
        serde_json::from_str(json).expect("parses")
    }

    #[test]
    fn session_cookies_drop_the_sentinel_expiry() {
        // Playwright writes -1 for "expires at end of session"; CDP reads that
        // literally as 1969 and would discard the cookie on arrival.
        let param = to_cookie_param(&cookie(
            r#"{"name":"s","value":"v","expires":-1,"domain":"x.test"}"#,
        ));
        assert!(param.expires.is_none());
    }

    #[test]
    fn real_expiries_survive() {
        let param = to_cookie_param(&cookie(
            r#"{"name":"s","value":"v","expires":1893456000,"domain":"x.test"}"#,
        ));
        assert!(param.expires.is_some());
    }

    #[test]
    fn same_site_maps_case_insensitively() {
        for (input, expected) in [
            (r#""Strict""#, Some(CookieSameSite::Strict)),
            (r#""lax""#, Some(CookieSameSite::Lax)),
            (r#""NONE""#, Some(CookieSameSite::None)),
        ] {
            let param = to_cookie_param(&cookie(&format!(
                r#"{{"name":"s","value":"v","sameSite":{input}}}"#
            )));
            assert_eq!(param.same_site, expected, "input {input}");
        }
    }

    #[test]
    fn unknown_same_site_is_dropped_not_fatal() {
        let param = to_cookie_param(&cookie(
            r#"{"name":"s","value":"v","sameSite":"Unspecified"}"#,
        ));
        assert!(param.same_site.is_none());
    }

    #[test]
    fn name_and_value_always_carry_over() {
        let param = to_cookie_param(&cookie(r#"{"name":"session","value":"abc123"}"#));
        assert_eq!(param.name, "session");
        assert_eq!(param.value, "abc123");
    }
}
