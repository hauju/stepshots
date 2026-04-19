use crate::browser::Browser;
use crate::config::StepConfig;
use crate::error::CliError;

/// Result of executing an action — may include transition frames for scroll actions.
pub struct ActionResult {
    /// Intermediate JPEG frames captured during smooth scroll transitions.
    pub transition_frames: Vec<Vec<u8>>,
}

/// Execute a step action against the browser.
/// Returns an `ActionResult` which may contain transition frames for scroll actions.
pub async fn execute_action(
    browser: &Browser,
    step: &StepConfig,
    base_url: &str,
) -> Result<ActionResult, CliError> {
    let mut result = ActionResult {
        transition_frames: vec![],
    };

    match step.action.as_str() {
        "click" => {
            let selector = step
                .selector
                .as_deref()
                .ok_or_else(|| CliError::Action("click action requires a selector".into()))?;
            if let Ok(el) = browser.page().find_element(selector).await {
                el.click()
                    .await
                    .map_err(|e| CliError::Action(format!("Click failed on '{selector}': {e}")))?;
            } else if let Some(bounds) = step.highlights.first().and_then(|h| h.bounds.clone()) {
                tracing::warn!(
                    "selector '{}' did not resolve for click step; using highlight bounds fallback",
                    selector
                );
                let click_x = bounds.x + bounds.width / 2.0;
                let click_y = bounds.y + bounds.height / 2.0;
                browser.click_at_point(click_x, click_y).await?;
            } else {
                return Err(CliError::Action(format!(
                    "Element not found '{selector}' and no highlight bounds were available for fallback click"
                )));
            }
        }
        "type" => {
            let selector = step
                .selector
                .as_deref()
                .ok_or_else(|| CliError::Action("type action requires a selector".into()))?;
            let text = step
                .text
                .as_deref()
                .ok_or_else(|| CliError::Action("type action requires text".into()))?;
            let el =
                browser.page().find_element(selector).await.map_err(|e| {
                    CliError::Action(format!("Element not found '{selector}': {e}"))
                })?;
            // Clear existing text first
            el.click()
                .await
                .map_err(|e| CliError::Action(format!("Focus failed on '{selector}': {e}")))?;
            browser
                .page()
                .execute(
                    chromiumoxide::cdp::browser_protocol::input::DispatchKeyEventParams::builder()
                        .r#type(
                            chromiumoxide::cdp::browser_protocol::input::DispatchKeyEventType::KeyDown,
                        )
                        .key("a")
                        .modifiers(if cfg!(target_os = "macos") { 4 } else { 2 })
                        .build()
                        .map_err(|e| CliError::Action(format!("Failed to build DispatchKeyEventParams: {e}")))?,
                )
                .await
                .ok();
            el.type_str(text)
                .await
                .map_err(|e| CliError::Action(format!("Type failed on '{selector}': {e}")))?;
        }
        "key" => {
            let key = step
                .key
                .as_deref()
                .ok_or_else(|| CliError::Action("key action requires a key".into()))?;
            browser
                .page()
                .find_element("body")
                .await
                .map_err(|e| CliError::Action(format!("Cannot find body: {e}")))?
                .press_key(key)
                .await
                .map_err(|e| CliError::Action(format!("Key press '{key}' failed: {e}")))?;
        }
        "scroll" => {
            let x = step.scroll_x.unwrap_or(0.0);
            let y = step.scroll_y.unwrap_or(0.0);

            // Use smooth scrolling and capture intermediate frames
            let js = if let Some(ref sel) = step.selector {
                format!(
                    "document.querySelector({}).scrollBy({{left:{x},top:{y},behavior:'smooth'}})",
                    serde_json::to_string(sel)?
                )
            } else {
                format!("window.scrollBy({{left:{x},top:{y},behavior:'smooth'}})")
            };
            browser
                .page()
                .evaluate(js)
                .await
                .map_err(|e| CliError::Action(format!("Scroll failed: {e}")))?;

            // Capture frames during the smooth scroll animation (~500ms),
            // skipping consecutive duplicate frames (scroll settled early).
            let frame_interval = tokio::time::Duration::from_millis(50);
            let max_frames = 12;
            for _ in 0..max_frames {
                tokio::time::sleep(frame_interval).await;
                match browser.screenshot_jpeg(70).await {
                    Ok(frame) => {
                        // Skip if identical to previous frame (scroll finished)
                        if result.transition_frames.last() == Some(&frame) {
                            continue;
                        }
                        result.transition_frames.push(frame);
                    }
                    Err(_) => break,
                }
            }
        }
        "scroll-to" => {
            let selector = step
                .highlight_selector
                .as_ref()
                .or(step.selector.as_ref())
                .ok_or_else(|| {
                    CliError::Action(
                        "scroll-to action requires a selector or highlightSelector".into(),
                    )
                })?;
            let js = format!(
                "(() => {{ const el = document.querySelector({sel}); if(el) el.scrollIntoView({{behavior:'smooth',block:'center'}}); }})()",
                sel = serde_json::to_string(selector)?
            );
            browser
                .page()
                .evaluate(js)
                .await
                .map_err(|e| CliError::Action(format!("scroll-to failed: {e}")))?;

            // Capture frames during the smooth scroll animation
            let frame_interval = tokio::time::Duration::from_millis(50);
            let max_frames = 12;
            for _ in 0..max_frames {
                tokio::time::sleep(frame_interval).await;
                match browser.screenshot_jpeg(70).await {
                    Ok(frame) => {
                        if result.transition_frames.last() == Some(&frame) {
                            continue;
                        }
                        result.transition_frames.push(frame);
                    }
                    Err(_) => break,
                }
            }
        }
        "hover" => {
            let selector = step
                .selector
                .as_deref()
                .ok_or_else(|| CliError::Action("hover action requires a selector".into()))?;
            let el =
                browser.page().find_element(selector).await.map_err(|e| {
                    CliError::Action(format!("Element not found '{selector}': {e}"))
                })?;
            el.scroll_into_view()
                .await
                .map_err(|e| CliError::Action(format!("Scroll into view failed: {e}")))?;
        }
        "navigate" => {
            let url = step
                .url
                .as_deref()
                .ok_or_else(|| CliError::Action("navigate action requires a url".into()))?;
            let full_url = resolve_url(base_url, url);
            browser.navigate(&full_url).await?;
        }
        "wait" => {
            if let Some(delay) = step.delay {
                tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
            }
            if let Some(ref selector) = step.selector {
                let timeout = std::time::Duration::from_secs(10);
                let start = std::time::Instant::now();
                loop {
                    let found = browser.page().find_element(selector).await;
                    if found.is_ok() {
                        break;
                    }
                    if start.elapsed() > timeout {
                        return Err(CliError::Action(format!(
                            "Timed out waiting for selector '{selector}'"
                        )));
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                }
            }
        }
        "select" => {
            let selector = step
                .selector
                .as_deref()
                .ok_or_else(|| CliError::Action("select action requires a selector".into()))?;
            let value = step
                .value
                .as_deref()
                .ok_or_else(|| CliError::Action("select action requires a value".into()))?;
            let js = format!(
                "(() => {{ const el = document.querySelector({sel}); if(el) {{ el.value = {val}; el.dispatchEvent(new Event('change', {{bubbles:true}})); }} }})()",
                sel = serde_json::to_string(selector)?,
                val = serde_json::to_string(value)?,
            );
            browser
                .page()
                .evaluate(js)
                .await
                .map_err(|e| CliError::Action(format!("Select failed: {e}")))?;
        }
        other => {
            return Err(CliError::Action(format!("Unknown action: {other}")));
        }
    }
    Ok(result)
}

/// Resolve a potentially relative URL against a base URL.
fn resolve_url(base: &str, url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        let base = base.trim_end_matches('/');
        let url = if url.starts_with('/') {
            url.to_string()
        } else {
            format!("/{url}")
        };
        format!("{base}{url}")
    }
}
