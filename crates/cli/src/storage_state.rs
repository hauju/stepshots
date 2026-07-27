//! Browser session state loaded from a JSON file, for recording authenticated
//! flows without a persistent profile.
//!
//! `--profile-dir` already solves authentication locally: log in once with
//! `stepshots browser`, and `record` reuses the session. That profile cannot
//! travel to CI, though — a Chrome user-data directory is bulky and binary, it
//! is tied to a machine and a Chrome version, it holds live session tokens so it
//! must never be committed, and the session inside it expires anyway.
//!
//! CI needs something a job can regenerate from scratch on every run. Playwright
//! already defines exactly that file — `storageState` — and any team running
//! browser tests can produce one from their existing login setup. So this reads
//! that format rather than inventing a competing one.
//!
//! Cookies are applied over CDP before the first navigation. `localStorage` is
//! restored by an init script that runs before page scripts on every document,
//! which avoids the extra navigation-per-origin dance a direct write would need.

use std::path::Path;

use serde::Deserialize;

use crate::error::CliError;

/// A Playwright-compatible `storageState` document.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageState {
    #[serde(default)]
    pub cookies: Vec<Cookie>,
    #[serde(default)]
    pub origins: Vec<OriginState>,
}

/// One cookie. Mirrors Playwright's shape; every field but `name`/`value` is
/// optional so a hand-written file stays short.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cookie {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    /// Seconds since the epoch. Playwright writes `-1` for session cookies;
    /// CDP wants the field omitted in that case, so non-positive values are
    /// dropped rather than sent.
    #[serde(default)]
    pub expires: Option<f64>,
    #[serde(default)]
    pub http_only: Option<bool>,
    #[serde(default)]
    pub secure: Option<bool>,
    /// "Strict", "Lax" or "None". Anything else is ignored rather than
    /// rejected — a session that mostly works beats a hard failure on a field
    /// Chrome itself treats as advisory.
    #[serde(default)]
    pub same_site: Option<String>,
}

/// `localStorage` for a single origin.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OriginState {
    pub origin: String,
    #[serde(default)]
    pub local_storage: Vec<LocalStorageEntry>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalStorageEntry {
    pub name: String,
    pub value: String,
}

impl StorageState {
    /// Read and parse a storage-state file.
    pub fn load(path: &Path) -> Result<Self, CliError> {
        let raw = std::fs::read_to_string(path).map_err(|e| {
            CliError::Config(format!(
                "Could not read storage state '{}': {e}. \
                 Generate one with Playwright's `context.storageState({{ path }})`, \
                 or write the JSON by hand — see `stepshots record --help`.",
                path.display()
            ))
        })?;

        let state: Self = serde_json::from_str(&raw).map_err(|e| {
            CliError::Config(format!(
                "Could not parse storage state '{}': {e}. \
                 Expected Playwright's storageState shape: \
                 {{\"cookies\": [...], \"origins\": [{{\"origin\": \"...\", \"localStorage\": [...]}}]}}.",
                path.display()
            ))
        })?;

        if state.is_empty() {
            return Err(CliError::Config(format!(
                "Storage state '{}' has no cookies and no localStorage entries. \
                 An empty session would record as logged-out, which is almost \
                 certainly not what you want — check the file was written after \
                 logging in.",
                path.display()
            )));
        }

        Ok(state)
    }

    pub fn is_empty(&self) -> bool {
        self.cookies.is_empty() && self.origins.iter().all(|o| o.local_storage.is_empty())
    }

    /// One-line description for progress output. Never includes values — a
    /// storage state is credentials in JSON form.
    pub fn summary(&self) -> String {
        let items: usize = self.origins.iter().map(|o| o.local_storage.len()).sum();
        format!(
            "{} cookie{}, {} localStorage entr{} across {} origin{}",
            self.cookies.len(),
            if self.cookies.len() == 1 { "" } else { "s" },
            items,
            if items == 1 { "y" } else { "ies" },
            self.origins.len(),
            if self.origins.len() == 1 { "" } else { "s" },
        )
    }

    /// JavaScript that repopulates `localStorage` for whichever of the known
    /// origins the document happens to be on. Returns `None` when there is
    /// nothing to restore.
    ///
    /// Runs on every new document, so it must be defensive: `about:blank` has
    /// origin `"null"`, and storage access throws outright when cookies are
    /// blocked for the site.
    pub fn local_storage_script(&self) -> Option<String> {
        let map: serde_json::Map<String, serde_json::Value> = self
            .origins
            .iter()
            .filter(|o| !o.local_storage.is_empty())
            .map(|o| {
                let entries: Vec<serde_json::Value> = o
                    .local_storage
                    .iter()
                    .map(|e| serde_json::json!([e.name, e.value]))
                    .collect();
                (o.origin.clone(), serde_json::Value::Array(entries))
            })
            .collect();

        if map.is_empty() {
            return None;
        }

        let json = serde_json::Value::Object(map).to_string();
        Some(format!(
            "(() => {{ try {{ const s = {json}; \
             const e = s[window.location.origin]; if (!e) return; \
             for (const [k, v] of e) window.localStorage.setItem(k, v); \
             }} catch (_) {{}} }})();"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> StorageState {
        serde_json::from_str(s).expect("parses")
    }

    #[test]
    fn parses_playwright_shape() {
        let state = parse(
            r#"{
                "cookies": [{
                    "name": "session", "value": "abc", "domain": "example.com",
                    "path": "/", "expires": -1, "httpOnly": true,
                    "secure": true, "sameSite": "Lax"
                }],
                "origins": [{
                    "origin": "https://example.com",
                    "localStorage": [{"name": "token", "value": "xyz"}]
                }]
            }"#,
        );
        assert_eq!(state.cookies.len(), 1);
        assert_eq!(state.cookies[0].name, "session");
        assert_eq!(state.cookies[0].same_site.as_deref(), Some("Lax"));
        assert_eq!(state.origins[0].local_storage[0].value, "xyz");
    }

    #[test]
    fn tolerates_a_minimal_hand_written_file() {
        let state = parse(r#"{"cookies": [{"name": "a", "value": "b"}]}"#);
        assert_eq!(state.cookies.len(), 1);
        assert!(state.origins.is_empty());
        assert!(!state.is_empty());
    }

    #[test]
    fn empty_state_is_detected() {
        assert!(parse(r#"{"cookies": [], "origins": []}"#).is_empty());
        // Origins present but carrying nothing is still empty.
        assert!(
            parse(r#"{"origins": [{"origin": "https://x.test", "localStorage": []}]}"#).is_empty()
        );
    }

    #[test]
    fn script_is_none_without_local_storage() {
        assert!(
            parse(r#"{"cookies": [{"name": "a", "value": "b"}]}"#)
                .local_storage_script()
                .is_none()
        );
    }

    #[test]
    fn script_embeds_entries_and_guards_origin() {
        let script = parse(
            r#"{"origins": [{"origin": "https://example.com",
                 "localStorage": [{"name": "k", "value": "v"}]}]}"#,
        )
        .local_storage_script()
        .expect("script");
        assert!(script.contains("https://example.com"));
        assert!(script.contains(r#"["k","v"]"#));
        assert!(script.contains("window.location.origin"));
        // Must not throw on about:blank or when storage is blocked.
        assert!(script.contains("catch"));
    }

    #[test]
    fn script_escapes_values_that_would_break_out_of_the_literal() {
        let script = parse(
            r#"{"origins": [{"origin": "https://example.com",
                 "localStorage": [{"name": "k", "value": "</script>\"'"}]}]}"#,
        )
        .local_storage_script()
        .expect("script");
        // serde_json is doing the escaping; the raw sequence must not appear.
        assert!(!script.contains("\"</script>\"'\""));
        assert!(script.contains("\\\""));
    }

    #[test]
    fn summary_reports_counts_without_values() {
        let state = parse(
            r#"{"cookies": [{"name": "s", "value": "SECRET"}],
                "origins": [{"origin": "https://x.test",
                  "localStorage": [{"name": "t", "value": "ALSO_SECRET"}]}]}"#,
        );
        let summary = state.summary();
        assert!(!summary.contains("SECRET"));
        assert!(!summary.contains("ALSO_SECRET"));
        assert!(summary.contains("1 cookie,"));
        assert!(summary.contains("1 localStorage entry"));
    }
}
