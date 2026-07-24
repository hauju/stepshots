//! `stepshots tour` — work with guided-tour source files (`*.tour.json`).
//!
//! A guided tour is its own git-versioned asset: `init` scaffolds a source
//! file (optionally projecting a recorded `.stepshot` bundle, whose selectors
//! and fallback anchors are the hard part to hand-author), `validate` checks
//! it statically, `check` (see `tour_check`) replays it against a live app,
//! and `build` merges source files into the `window.__STEPSHOTS_TOURS`
//! registry script for script-tag installs.
//!
//! `export` is the deprecated predecessor (bundle → track in one shot);
//! `init --from` + `build` replace it.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use clap::ValueEnum;
use manifest::{BundleManifest, TOUR_FILE_SCHEMA_VERSION, TourAdvance, TourFile, TourTrack};

use crate::error::CliError;

/// Filename suffix identifying tour source files.
pub const TOUR_FILE_SUFFIX: &str = ".tour.json";

/// Output format for `tour export`.
#[derive(Copy, Clone, ValueEnum)]
pub enum TourFormat {
    /// A `{ "<key>": { "steps": [...] } }` registry (fetch, import, or inline).
    Json,
    /// A script that assigns `window.__STEPSHOTS_TOURS` (drop-in `<script src>`).
    Js,
}

fn read_manifest(bundle: &Path) -> Result<BundleManifest, CliError> {
    let bytes = std::fs::read(bundle)?;
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)?;
    let mut entry = archive.by_name("manifest.json").map_err(|_| {
        CliError::Bundle(format!("{}: no manifest.json in bundle", bundle.display()))
    })?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf)?;
    Ok(serde_json::from_slice(&buf)?)
}

/// Export a tour track from `bundle` to `output` (default `<bundle>.tour.json`),
/// registered under `key` (default the bundle's filename stem). The output is the
/// `{ "<key>": { "steps": [...] } }` registry shape `window.__STEPSHOTS_TOURS` expects.
pub fn export(
    bundle: &Path,
    output: Option<PathBuf>,
    key: Option<String>,
    format: TourFormat,
    json: bool,
) -> Result<(), CliError> {
    if !json {
        eprintln!(
            "note: `tour export` is deprecated — use `tour init --from <bundle>` to scaffold a \
             source file, or `tour build` for the registry script."
        );
    }
    let stem = bundle
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("tour")
        .to_string();
    let key = key.unwrap_or(stem);
    let default_ext = match format {
        TourFormat::Json => "tour.json",
        TourFormat::Js => "tour.js",
    };
    let out_path = output.unwrap_or_else(|| bundle.with_extension(default_ext));

    let manifest = read_manifest(bundle)?;
    let track = manifest.to_tour_track();

    if track.steps.is_empty() {
        return Err(CliError::Bundle(format!(
            "{}: no tour steps derived — recorded steps need a highlight callout + a click/type action",
            bundle.display()
        )));
    }

    let step_count = track.steps.len();
    let mut registry: BTreeMap<String, TourTrack> = BTreeMap::new();
    registry.insert(key.clone(), track);
    let registry_json = serde_json::to_string_pretty(&registry)?;
    // JS format merges into any existing registry so multiple exports can coexist.
    let contents = match format {
        TourFormat::Json => format!("{registry_json}\n"),
        TourFormat::Js => format!(
            "window.__STEPSHOTS_TOURS = Object.assign(window.__STEPSHOTS_TOURS || {{}}, {registry_json});\n"
        ),
    };
    std::fs::write(&out_path, contents)?;

    if json {
        let out = serde_json::json!({
            "success": true,
            "command": "tour export",
            "bundle": bundle.display().to_string(),
            "output": out_path.display().to_string(),
            "key": key,
            "steps": step_count,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&out).expect("serializing tour export output")
        );
    } else {
        println!(
            "Exported tour \"{key}\" ({step_count} steps) → {}",
            out_path.display()
        );
        for track in registry.values() {
            for s in &track.steps {
                println!(
                    "  - {:<5} {}  \"{}\"",
                    s.advance.as_str(),
                    s.selector,
                    s.title
                );
            }
        }
    }

    Ok(())
}

/// Push tour source files to the dashboard: upsert hosted tours by `key`.
/// One-way sync — git stays the source of truth, the hosted copy is
/// overwritten, never merged. Check hints are stripped; only the runtime
/// track plus metadata is sent.
pub async fn push(
    paths: &[PathBuf],
    server_url: &str,
    token: &str,
    json: bool,
) -> Result<(), CliError> {
    let files = collect_tour_files(paths)?;
    let client = reqwest::Client::new();
    let server = server_url.trim_end_matches('/');
    let mut pushed: Vec<serde_json::Value> = Vec::new();
    let mut skipped: Vec<serde_json::Value> = Vec::new();

    for path in &files {
        let file = load_tour_file(path)
            .map_err(|e| CliError::Config(format!("{}: {e}", path.display())))?;
        // Hosted tours serve the default locale; a variant pushed under the
        // same key would overwrite it. Localized registries are self-hosted
        // via `tour build --locale`.
        if let Some(locale) = &file.locale {
            if !json {
                println!(
                    "Skipped \"{}\" [{locale}] — hosted tours serve the default locale; build a \
                     localized registry with `stepshots tour build --locale {locale}`",
                    file.key
                );
            }
            skipped.push(serde_json::json!({
                "path": path.display().to_string(),
                "key": file.key,
                "locale": locale,
            }));
            continue;
        }
        let track = file.to_track();
        if track.steps.is_empty() {
            return Err(CliError::Config(format!(
                "{}: tour has no steps",
                path.display()
            )));
        }

        let body = serde_json::json!({
            "schema": file.schema,
            "key": file.key,
            "title": file.title,
            "steps": track.steps,
        });
        let resp = client
            .put(format!("{server}/api/tours/push"))
            .header("Authorization", format!("Bearer {token}"))
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(push_error(&file.key, resp).await);
        }
        let response: serde_json::Value = resp.json().await?;
        let id = response
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| CliError::Upload("API response missing 'id' field".into()))?
            .to_string();
        let created = response
            .get("created")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if !json {
            println!(
                "{} \"{}\" ({} steps)",
                if created { "Created" } else { "Updated" },
                file.key,
                track.steps.len()
            );
            println!("  Track: {server}/api/tours/public/{id}/track");
            println!(
                "  Embed: <script src=\"{server}/tour.js\" data-stepshots-tour=\"{id}\" defer></script>"
            );
        }
        pushed.push(serde_json::json!({
            "path": path.display().to_string(),
            "key": file.key,
            "id": id,
            "created": created,
            "steps": track.steps.len(),
        }));
    }

    if json {
        let out = serde_json::json!({
            "success": true,
            "command": "tour push",
            "server": server,
            "tours": pushed,
            "skipped": skipped,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&out).expect("serializing tour push output")
        );
    }
    Ok(())
}

/// Turn a non-success push response into a helpful error, mirroring upload's
/// 401 handling.
async fn push_error(key: &str, resp: reqwest::Response) -> CliError {
    let status = resp.status();
    if status.as_u16() == 401 {
        return CliError::Auth(
            "API token is invalid or expired. Run `stepshots login` again, \
             or check --token / STEPSHOTS_TOKEN."
                .into(),
        );
    }
    let body = resp.text().await.unwrap_or_default();
    let message = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(String::from))
        .unwrap_or(body);
    CliError::Upload(format!("Push \"{key}\" failed ({status}): {message}"))
}

/// Scaffold a guided-tour source file at `tours/<key>.tour.json` (or `output`).
/// With `from`, projects the recorded bundle once — after that the tour file is
/// independent of the recording. With `locale`, scaffolds a translated variant
/// at `tours/<key>.<locale>.tour.json`, copying the base file's structure when
/// it exists so only the strings need translating.
pub fn init(
    key: &str,
    from: Option<&Path>,
    locale: Option<&str>,
    output: Option<PathBuf>,
    force: bool,
    json: bool,
) -> Result<(), CliError> {
    if key.is_empty() || key.chars().any(char::is_whitespace) {
        return Err(CliError::Config(
            "tour key must be non-empty and contain no whitespace (it becomes the \
             ?tour= URL parameter)"
                .into(),
        ));
    }
    if let Some(tag) = locale
        && !valid_locale(tag)
    {
        return Err(CliError::Config(format!(
            "locale \"{tag}\" is not a language tag (expected e.g. \"de\" or \"fr-CA\")"
        )));
    }
    let out_path = output.unwrap_or_else(|| match locale {
        Some(tag) => PathBuf::from(format!("tours/{key}.{tag}{TOUR_FILE_SUFFIX}")),
        None => PathBuf::from(format!("tours/{key}{TOUR_FILE_SUFFIX}")),
    });
    if out_path.exists() && !force {
        return Err(CliError::Config(format!(
            "{} already exists. Use --force to overwrite.",
            out_path.display()
        )));
    }

    let title = humanize_key(key);
    let base_path = PathBuf::from(format!("tours/{key}{TOUR_FILE_SUFFIX}"));
    // Track where the scaffold came from for the follow-up hints below.
    let mut copied_base = false;
    let mut file = match from {
        Some(bundle) => {
            let manifest = read_manifest(bundle)?;
            let track = manifest.to_tour_track();
            if track.steps.is_empty() {
                return Err(CliError::Bundle(format!(
                    "{}: no tour steps derived — recorded steps need a highlight callout + a click/type/select action",
                    bundle.display()
                )));
            }
            TourFile::from_track(track, key.to_string(), title)
        }
        // A locale variant starts as a copy of the base file when one exists:
        // same selectors/advance/check, strings left in place to translate.
        None if locale.is_some() && base_path.is_file() => {
            copied_base = true;
            load_tour_file(&base_path)
                .map_err(|e| CliError::Config(format!("{}: {e}", base_path.display())))?
        }
        None => template_file(key, title),
    };
    file.locale = locale.map(String::from);

    if let Some(parent) = out_path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        &out_path,
        format!("{}\n", serde_json::to_string_pretty(&file)?),
    )?;

    if json {
        let out = serde_json::json!({
            "success": true,
            "command": "tour init",
            "output": out_path.display().to_string(),
            "key": key,
            "locale": locale,
            "steps": file.steps.len(),
            "from": from.map(|p| p.display().to_string()),
            "copiedBase": copied_base,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&out).expect("serializing tour init output")
        );
    } else {
        let locale_label = locale.map(|l| format!(" [{l}]")).unwrap_or_default();
        println!(
            "Created tour \"{key}\"{locale_label} ({} steps) → {}",
            file.steps.len(),
            out_path.display()
        );
        if copied_base {
            println!(
                "Translate each step's title and body (selectors and advance stay in sync \
                 with {}), then:",
                base_path.display()
            );
        } else if from.is_none() {
            println!("Edit the placeholder selectors and copy, then:");
        } else {
            println!("Review the projected copy (recordings sell; tours instruct), then:");
        }
        println!("  stepshots tour validate {}", out_path.display());
        if locale.is_some() {
            // The copied fallback anchors are the base language's button text —
            // refresh them from the localized UI so drift recovery works there.
            println!(
                "  stepshots tour check --url <localized-staging-url> --update-fallbacks {}",
                out_path.display()
            );
        } else {
            println!(
                "  stepshots tour check --url <staging-url> {}",
                out_path.display()
            );
        }
    }
    Ok(())
}

/// Statically validate tour source files: strict parse + lints. Exits
/// non-zero when any file has errors; warnings are advisory. Identity is
/// (key, locale) — a locale variant is additionally checked against its
/// default-locale base so stale translations fail CI.
pub fn validate(paths: &[PathBuf], json: bool) -> Result<(), CliError> {
    let files = collect_tour_files(paths)?;
    let mut seen_keys: BTreeMap<(String, Option<String>), PathBuf> = BTreeMap::new();
    let mut reports = Vec::with_capacity(files.len());
    let mut parsed: Vec<(usize, TourFile)> = Vec::new();

    for path in &files {
        let mut errors: Vec<String> = Vec::new();
        let mut warnings: Vec<String> = Vec::new();
        let mut key = None;
        let mut locale = None;
        let mut steps = 0;
        match load_tour_file(path) {
            Err(e) => errors.push(e),
            Ok(file) => {
                lint_tour_file(&file, &mut errors, &mut warnings);
                let identity = (file.key.clone(), file.locale.clone());
                if let Some(prev) = seen_keys.insert(identity, path.clone()) {
                    errors.push(format!(
                        "duplicate tour key \"{}\"{} (also used by {})",
                        file.key,
                        file.locale
                            .as_deref()
                            .map(|l| format!(" for locale \"{l}\""))
                            .unwrap_or_default(),
                        prev.display()
                    ));
                }
                key = Some(file.key.clone());
                locale = file.locale.clone();
                steps = file.steps.len();
                parsed.push((reports.len(), file));
            }
        }
        reports.push((path.clone(), key, locale, steps, errors, warnings));
    }

    // Cross-file pass: every locale variant must track its default-locale base.
    let bases: BTreeMap<&str, &TourFile> = parsed
        .iter()
        .filter(|(_, f)| f.locale.is_none())
        .map(|(_, f)| (f.key.as_str(), f))
        .collect();
    for (idx, file) in &parsed {
        if file.locale.is_none() {
            continue;
        }
        let report = &mut reports[*idx];
        match bases.get(file.key.as_str()) {
            Some(base) => lint_locale_variant(base, file, &mut report.4, &mut report.5),
            None => report.5.push(format!(
                "no default-locale file for key \"{}\" — `tour build` without --locale won't \
                 include this tour",
                file.key
            )),
        }
    }

    let error_count: usize = reports.iter().map(|r| r.4.len()).sum();
    let warn_count: usize = reports.iter().map(|r| r.5.len()).sum();

    if json {
        let out = serde_json::json!({
            "success": error_count == 0,
            "command": "tour validate",
            "files": reports.iter().map(|(path, key, locale, steps, errors, warnings)| {
                serde_json::json!({
                    "path": path.display().to_string(),
                    "key": key,
                    "locale": locale,
                    "steps": steps,
                    "status": if !errors.is_empty() { "error" }
                              else if !warnings.is_empty() { "warn" } else { "ok" },
                    "errors": errors,
                    "warnings": warnings,
                })
            }).collect::<Vec<_>>(),
            "summary": { "files": reports.len(), "errors": error_count, "warnings": warn_count },
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&out).expect("serializing tour validate output")
        );
    } else {
        for (path, key, locale, steps, errors, warnings) in &reports {
            let label = match (key.as_deref(), locale.as_deref()) {
                (Some(k), Some(l)) => format!("{k} [{l}]"),
                (Some(k), None) => k.to_string(),
                _ => "?".to_string(),
            };
            if !errors.is_empty() {
                println!("✗ {} — {label}", path.display());
            } else if !warnings.is_empty() {
                println!("⚠ {} — {label}, {steps} step(s)", path.display());
            } else {
                println!("✓ {} — {label}, {steps} step(s)", path.display());
            }
            for e in errors {
                println!("    error: {e}");
            }
            for w in warnings {
                println!("    warning: {w}");
            }
        }
        println!(
            "\n{} file(s) · {} error(s) · {} warning(s)",
            reports.len(),
            error_count,
            warn_count
        );
    }

    if error_count > 0 {
        return Err(CliError::Reported { code: 1 });
    }
    Ok(())
}

/// Merge tour source files into a `window.__STEPSHOTS_TOURS` registry script
/// for script-tag installs. Bundler users import the `.tour.json` directly.
/// Without `locale`, only default-locale files are included; with `locale`,
/// each key resolves to its matching variant, falling back to the base — the
/// app then includes the registry that matches its own UI language.
pub fn build(
    paths: &[PathBuf],
    output: &Path,
    locale: Option<&str>,
    json: bool,
) -> Result<(), CliError> {
    if let Some(tag) = locale
        && !valid_locale(tag)
    {
        return Err(CliError::Config(format!(
            "locale \"{tag}\" is not a language tag (expected e.g. \"de\" or \"fr-CA\")"
        )));
    }
    let paths = collect_tour_files(paths)?;
    let mut files: Vec<(PathBuf, TourFile)> = Vec::with_capacity(paths.len());
    for path in &paths {
        let file = load_tour_file(path)
            .map_err(|e| CliError::Config(format!("{}: {e}", path.display())))?;
        files.push((path.clone(), file));
    }
    let selected = select_for_locale(&files, locale)?;
    let variant_files = files.iter().filter(|(_, f)| f.locale.is_some()).count();

    let mut registry: BTreeMap<String, TourTrack> = BTreeMap::new();
    for (key, file) in &selected {
        registry.insert(key.clone(), file.to_track());
    }

    let registry_json = serde_json::to_string_pretty(&registry)?;
    let contents = format!(
        "window.__STEPSHOTS_TOURS = Object.assign(window.__STEPSHOTS_TOURS || {{}}, {registry_json});\n"
    );
    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output, contents)?;

    if json {
        let out = serde_json::json!({
            "success": true,
            "command": "tour build",
            "output": output.display().to_string(),
            "locale": locale,
            "tours": selected.iter().map(|(key, file)| serde_json::json!({
                "key": key,
                "locale": file.locale,
                "steps": file.steps.len(),
            })).collect::<Vec<_>>(),
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&out).expect("serializing tour build output")
        );
    } else {
        println!("Built {} tour(s) → {}", registry.len(), output.display());
        for (key, file) in &selected {
            let resolved = match (locale, file.locale.as_deref()) {
                (Some(_), Some(l)) => format!(", {l}"),
                (Some(_), None) => ", default locale".to_string(),
                (None, _) => String::new(),
            };
            println!("  - {key} ({} steps{resolved})", file.steps.len());
        }
        if locale.is_none() && variant_files > 0 {
            println!(
                "  ({variant_files} locale variant file(s) not included — build them with \
                 `--locale <tag>`)"
            );
        }
    }
    Ok(())
}

/// Pick one file per key for a build: the requested locale's variant when
/// present, the default-locale file otherwise. Variants of other locales are
/// left out — each locale gets its own registry build.
fn select_for_locale<'a>(
    files: &'a [(PathBuf, TourFile)],
    locale: Option<&str>,
) -> Result<BTreeMap<String, &'a TourFile>, CliError> {
    // A duplicate (key, locale) pair is an authoring error regardless of the
    // requested locale — catch it before selection silently drops one.
    let mut seen: BTreeMap<(&str, Option<&str>), &Path> = BTreeMap::new();
    for (path, file) in files {
        if let Some(prev) = seen.insert((file.key.as_str(), file.locale.as_deref()), path) {
            return Err(CliError::Config(format!(
                "duplicate tour key \"{}\"{} in {} and {}",
                file.key,
                file.locale
                    .as_deref()
                    .map(|l| format!(" for locale \"{l}\""))
                    .unwrap_or_default(),
                prev.display(),
                path.display()
            )));
        }
    }
    let mut selected: BTreeMap<String, &TourFile> = BTreeMap::new();
    if let Some(want) = locale {
        for (_, file) in files {
            if file.locale.as_deref() == Some(want) {
                selected.insert(file.key.clone(), file);
            }
        }
    }
    for (_, file) in files {
        if file.locale.is_none() {
            selected.entry(file.key.clone()).or_insert(file);
        }
    }
    Ok(selected)
}

/// After recording a `"target": "tour"` tutorial: warn about interactive
/// steps the tour projection drops, and scaffold `tours/<key>.tour.json`
/// when it doesn't exist yet. An existing tour file is never touched — it's
/// the hand-edited source of truth.
pub fn post_record(key: &str, bundle: &Path, json: bool) -> Result<(), CliError> {
    let manifest = read_manifest(bundle)?;
    for (i, step) in manifest.steps.iter().enumerate() {
        let interactive = step
            .action
            .as_deref()
            .and_then(TourAdvance::from_action)
            .is_some();
        let has_callout = step
            .highlights
            .as_ref()
            .is_some_and(|hs| hs.iter().any(|h| h.callout.is_some()));
        if interactive && !has_callout {
            eprintln!(
                "  tour warning: step {} ({}) has no highlight callout — it won't appear in the tour",
                i + 1,
                step.selector.as_deref().unwrap_or("?")
            );
        }
    }

    let track = manifest.to_tour_track();
    if track.steps.is_empty() {
        eprintln!(
            "  tour warning: no tour steps derived — interactive steps need highlight callouts"
        );
        return Ok(());
    }

    let out_path = PathBuf::from(format!("tours/{key}{TOUR_FILE_SUFFIX}"));
    if out_path.exists() {
        if !json {
            println!(
                "  Tour file {} exists — left untouched (re-scaffold with `stepshots tour init {key} --from {} --force`)",
                out_path.display(),
                bundle.display()
            );
        }
        return Ok(());
    }

    let file = TourFile::from_track(track, key.to_string(), humanize_key(key));
    if let Some(parent) = out_path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        &out_path,
        format!("{}\n", serde_json::to_string_pretty(&file)?),
    )?;
    if !json {
        println!(
            "  Scaffolded guided tour → {} ({} steps)",
            out_path.display(),
            file.steps.len()
        );
    }
    Ok(())
}

/// Resolve which tour files to operate on: explicit files and directories,
/// or the `tours/` directory when nothing is given.
pub fn collect_tour_files(paths: &[PathBuf]) -> Result<Vec<PathBuf>, CliError> {
    let mut files = Vec::new();
    if paths.is_empty() {
        let dir = Path::new("tours");
        if !dir.is_dir() {
            return Err(CliError::Config(
                "no tour files given and no tours/ directory found — pass *.tour.json paths, \
                 or scaffold one with `stepshots tour init <key>`"
                    .into(),
            ));
        }
        collect_from_dir(dir, &mut files)?;
    } else {
        for p in paths {
            if p.is_dir() {
                collect_from_dir(p, &mut files)?;
            } else {
                files.push(p.clone());
            }
        }
    }
    if files.is_empty() {
        return Err(CliError::Config("no *.tour.json files found".into()));
    }
    files.sort();
    files.dedup();
    Ok(files)
}

fn collect_from_dir(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), CliError> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        let is_tour_file = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with(TOUR_FILE_SUFFIX));
        if path.is_file() && is_tour_file {
            out.push(path);
        }
    }
    Ok(())
}

/// Strict parse of a tour source file (unknown fields are rejected).
pub fn load_tour_file(path: &Path) -> Result<TourFile, String> {
    let contents = std::fs::read_to_string(path).map_err(|e| format!("cannot read file: {e}"))?;
    serde_json::from_str(&contents).map_err(|e| format!("not a valid tour file: {e}"))
}

/// Lints beyond what the schema can express.
fn lint_tour_file(file: &TourFile, errors: &mut Vec<String>, warnings: &mut Vec<String>) {
    if file.schema != TOUR_FILE_SCHEMA_VERSION {
        errors.push(format!(
            "unsupported schema version \"{}\" — this CLI supports \"{TOUR_FILE_SCHEMA_VERSION}\" \
             (run `stepshots upgrade`?)",
            file.schema
        ));
    }
    if file.key.is_empty() || file.key.chars().any(char::is_whitespace) {
        errors.push("key must be non-empty and contain no whitespace".into());
    }
    if let Some(locale) = &file.locale
        && !valid_locale(locale)
    {
        errors.push(format!(
            "locale \"{locale}\" is not a language tag (expected e.g. \"de\" or \"fr-CA\")"
        ));
    }
    if file.steps.is_empty() {
        errors.push("a tour needs at least one step".into());
    }
    let mut prev_selector: Option<&str> = None;
    for (i, step) in file.steps.iter().enumerate() {
        let n = i + 1;
        if step.selector.trim().is_empty() {
            errors.push(format!("step {n}: selector is empty"));
        }
        if step.title.trim().is_empty() {
            warnings.push(format!("step {n}: title is empty (callout heading)"));
        }
        if step.body.trim().is_empty() {
            warnings.push(format!("step {n}: body is empty (callout copy)"));
        }
        if prev_selector == Some(step.selector.as_str()) {
            warnings.push(format!(
                "step {n}: same selector as the previous step — did you mean to merge them?"
            ));
        }
        prev_selector = Some(step.selector.as_str());
        let check_value = step.check.as_ref().and_then(|c| c.value.as_deref());
        match step.advance {
            TourAdvance::Click => {
                if check_value.is_some() {
                    warnings.push(format!("step {n}: check.value is ignored for click steps"));
                }
            }
            TourAdvance::Input => {
                if check_value.is_none() {
                    warnings.push(format!(
                        "step {n}: no check.value — `tour check` will type a placeholder"
                    ));
                }
            }
            TourAdvance::Change => {
                if check_value.is_none() {
                    warnings.push(format!(
                        "step {n}: no check.value — `tour check` will pick the first option"
                    ));
                }
            }
        }
        if step.fallback.as_ref().is_some_and(|f| f.is_empty()) {
            warnings.push(format!(
                "step {n}: fallback carries no text or aria anchor — drop it, or run \
                 `tour check --update-fallbacks` to capture anchors"
            ));
        }
    }
}

/// Loose language-tag check ("de", "fr-CA", "zh-Hant"): a 2–3 letter primary
/// subtag plus optional alphanumeric subtags — enough to catch typos without
/// re-implementing BCP 47.
fn valid_locale(tag: &str) -> bool {
    let mut parts = tag.split('-');
    let primary = parts.next().unwrap_or("");
    if !(2..=3).contains(&primary.len()) || !primary.chars().all(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    parts.all(|p| (1..=8).contains(&p.len()) && p.chars().all(|c| c.is_ascii_alphanumeric()))
}

/// Compare a locale variant against its default-locale base. A translation
/// should only change the user-visible strings (title, body, fallback
/// anchors) — structure drift means the base changed since the translation
/// was written, which is exactly what CI should catch.
fn lint_locale_variant(
    base: &TourFile,
    variant: &TourFile,
    errors: &mut Vec<String>,
    warnings: &mut Vec<String>,
) {
    let locale = variant.locale.as_deref().unwrap_or("?");
    if base.steps.len() != variant.steps.len() {
        errors.push(format!(
            "translation out of date: {} step(s), but the default-locale file has {} — re-sync \
             the \"{locale}\" variant with the base",
            variant.steps.len(),
            base.steps.len()
        ));
        return;
    }
    for (i, (b, v)) in base.steps.iter().zip(&variant.steps).enumerate() {
        let n = i + 1;
        if b.selector != v.selector {
            warnings.push(format!(
                "step {n}: selector differs from the default-locale file (\"{}\" vs \"{}\") — \
                 fine only if the {locale} UI really renders different markup",
                v.selector, b.selector
            ));
        }
        if b.advance != v.advance {
            warnings.push(format!(
                "step {n}: advance differs from the default-locale file — a translation should \
                 only change the copy"
            ));
        }
    }
}

/// "getting-started" → "Getting started".
fn humanize_key(key: &str) -> String {
    let spaced = key.replace(['-', '_'], " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Blank two-step template showing the shape of a tour file.
fn template_file(key: &str, title: String) -> TourFile {
    serde_json::from_value(serde_json::json!({
        "$schema": manifest::TOUR_SCHEMA_URL,
        "schema": TOUR_FILE_SCHEMA_VERSION,
        "key": key,
        "title": title,
        "steps": [
            {
                "selector": "#replace-me",
                "title": "First step",
                "body": "Point the selector at the element users interact with first, then explain what to do.",
                "advance": { "type": "click" }
            },
            {
                "selector": "input[name=replace-me]",
                "title": "Second step",
                "body": "Input steps advance once the user has typed a value.",
                "advance": { "type": "input" },
                "check": { "value": "Example value typed by `stepshots tour check`" }
            }
        ]
    }))
    .expect("template is a valid tour file")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_from(json: serde_json::Value) -> TourFile {
        serde_json::from_value(json).expect("valid tour file")
    }

    fn lint(file: &TourFile) -> (Vec<String>, Vec<String>) {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        lint_tour_file(file, &mut errors, &mut warnings);
        (errors, warnings)
    }

    #[test]
    fn template_passes_its_own_lints() {
        let (errors, warnings) = lint(&template_file("demo", "Demo".into()));
        assert!(errors.is_empty(), "template must lint clean: {errors:?}");
        assert!(
            warnings.is_empty(),
            "template must lint clean: {warnings:?}"
        );
    }

    #[test]
    fn lints_flag_bad_schema_key_and_empty_steps() {
        let file = file_from(serde_json::json!({
            "schema": "2", "key": "has space", "title": "T", "steps": []
        }));
        let (errors, _) = lint(&file);
        assert_eq!(errors.len(), 3, "{errors:?}");
    }

    #[test]
    fn lints_flag_step_copy_and_check_hints() {
        let file = file_from(serde_json::json!({
            "schema": "1", "key": "k", "title": "T",
            "steps": [
                { "selector": "#a", "title": "", "body": "b",
                  "advance": { "type": "click" }, "check": { "value": "ignored" } },
                { "selector": "#a", "title": "t", "body": "b",
                  "advance": { "type": "input" } }
            ]
        }));
        let (errors, warnings) = lint(&file);
        assert!(errors.is_empty(), "{errors:?}");
        // empty title, click-with-value, duplicate selector, input-without-value
        assert_eq!(warnings.len(), 4, "{warnings:?}");
    }

    #[test]
    fn humanize_key_capitalizes_and_spaces() {
        assert_eq!(humanize_key("getting-started"), "Getting started");
        assert_eq!(humanize_key("a_b"), "A b");
    }

    #[test]
    fn valid_locale_accepts_tags_and_rejects_junk() {
        for ok in ["de", "fr-CA", "zh-Hant", "pt-BR", "es-419"] {
            assert!(valid_locale(ok), "{ok} should be valid");
        }
        for bad in [
            "",
            "d",
            "german",
            "de_DE",
            "de-",
            "-de",
            "de-toolongsubtag1",
        ] {
            assert!(!valid_locale(bad), "{bad} should be invalid");
        }
    }

    #[test]
    fn lints_flag_bad_locale() {
        let file = file_from(serde_json::json!({
            "schema": "1", "key": "k", "locale": "german!", "title": "T",
            "steps": [{ "selector": "#a", "title": "t", "body": "b",
                        "advance": { "type": "click" } }]
        }));
        let (errors, _) = lint(&file);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("language tag"), "{errors:?}");
    }

    fn base_and_variant() -> (TourFile, TourFile) {
        let base = file_from(serde_json::json!({
            "schema": "1", "key": "onboarding", "title": "Getting started",
            "steps": [
                { "selector": "#new", "title": "Create", "body": "Click.",
                  "advance": { "type": "click" }, "fallback": { "text": "New project" } },
                { "selector": "#name", "title": "Name", "body": "Type.",
                  "advance": { "type": "input" }, "check": { "value": "x" } }
            ]
        }));
        let variant = file_from(serde_json::json!({
            "schema": "1", "key": "onboarding", "locale": "de", "title": "Erste Schritte",
            "steps": [
                { "selector": "#new", "title": "Erstellen", "body": "Klicken.",
                  "advance": { "type": "click" }, "fallback": { "text": "Neues Projekt" } },
                { "selector": "#name", "title": "Benennen", "body": "Tippen.",
                  "advance": { "type": "input" }, "check": { "value": "x" } }
            ]
        }));
        (base, variant)
    }

    fn cross_lint(base: &TourFile, variant: &TourFile) -> (Vec<String>, Vec<String>) {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        lint_locale_variant(base, variant, &mut errors, &mut warnings);
        (errors, warnings)
    }

    #[test]
    fn translated_variant_with_matching_structure_lints_clean() {
        let (base, variant) = base_and_variant();
        let (errors, warnings) = cross_lint(&base, &variant);
        assert!(errors.is_empty(), "{errors:?}");
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    #[test]
    fn stale_translation_step_count_is_an_error() {
        let (mut base, variant) = base_and_variant();
        base.steps.push(base.steps[0].clone()); // base grew since translation
        let (errors, _) = cross_lint(&base, &variant);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("translation out of date"), "{errors:?}");
    }

    #[test]
    fn structure_drift_in_a_variant_warns() {
        let (base, mut variant) = base_and_variant();
        variant.steps[0].selector = "#other".into();
        variant.steps[1].advance = TourAdvance::Change;
        let (errors, warnings) = cross_lint(&base, &variant);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(warnings.len(), 2, "{warnings:?}");
    }

    #[test]
    fn select_for_locale_prefers_variant_and_falls_back() {
        let (base, variant) = base_and_variant();
        let other = file_from(serde_json::json!({
            "schema": "1", "key": "billing", "title": "Billing",
            "steps": [{ "selector": "#b", "title": "t", "body": "b",
                        "advance": { "type": "click" } }]
        }));
        let files = vec![
            (PathBuf::from("tours/onboarding.tour.json"), base),
            (PathBuf::from("tours/onboarding.de.tour.json"), variant),
            (PathBuf::from("tours/billing.tour.json"), other),
        ];

        // Default build: base files only — the de variant never collides.
        let default = select_for_locale(&files, None).unwrap();
        assert_eq!(default.len(), 2);
        assert_eq!(default["onboarding"].locale, None);

        // de build: variant wins for onboarding, billing falls back to base.
        let de = select_for_locale(&files, Some("de")).unwrap();
        assert_eq!(de.len(), 2);
        assert_eq!(de["onboarding"].locale.as_deref(), Some("de"));
        assert_eq!(de["billing"].locale, None);

        // fr build: no fr variants anywhere — everything falls back.
        let fr = select_for_locale(&files, Some("fr")).unwrap();
        assert_eq!(fr["onboarding"].locale, None);
    }

    #[test]
    fn select_for_locale_rejects_duplicate_identity() {
        let (base, _) = base_and_variant();
        let files = vec![
            (PathBuf::from("a.tour.json"), base.clone()),
            (PathBuf::from("b.tour.json"), base),
        ];
        let err = select_for_locale(&files, None).unwrap_err();
        assert!(err.to_string().contains("duplicate tour key"), "{err}");
    }
}
