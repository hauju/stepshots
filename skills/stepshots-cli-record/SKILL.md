---
name: stepshots-cli-record
description: |
  Create, record, verify, and publish screenshot-based product demos with the Stepshots
  CLI from natural-language flow descriptions. Use when a user wants to record a
  clickthrough demo or product tour, generate or edit stepshots.config.json, upload/publish
  a demo for sharing or embedding, check whether recorded demos are still up to date
  (demo drift detection with `stepshots verify`), repair a drifted demo, keep a demo
  fresh from CI, or turn an existing saved Stepshots demo into a CLI recording config.
  Also covers guided tours: recording with `"target": "tour"`, authoring/validating
  tours/<key>.tour.json files, and `stepshots tour init/validate/check/build`.
  Works for both CLI-first and AI-assisted workflows.
author: Hauke Jung
version: 2.4.0
---

# Stepshots CLI Screenshot Demo Skill

Use this skill when the user wants a screenshot clickthrough demo, not a live HTML replay.

This skill is adapted to the current product model:
- screenshot demos are the primary path
- click steps are target-first
- the extension records explicit actions only
- `Capture Screen` creates a screenshot-only step
- old auto-recorded navigation assumptions are no longer the recommended model

## What You Should Do

When using this skill, you should:

1. Understand the flow the user wants to show.
2. Prefer a clean clickthrough sequence over trying to show every intermediate UI motion.
3. Generate or update `stepshots.config.json`.
4. Prefer `click`, `type`, `select`, `key`, `wait`, and explicit scene captures.
5. Use the CLI to preview, validate, and record.
6. Upload the result so the user gets a shareable, embeddable demo — recording without
   publishing leaves the job half done.
7. If the user already has a saved demo, export or reconstruct a CLI config from that demo when possible.

## End-to-End Workflow

The full loop from nothing to a published demo:

```bash
stepshots login                          # once per machine (browser OAuth, token stored locally)
stepshots init                           # scaffold stepshots.config.json (skip if one exists)
stepshots inspect https://example.com --json   # discover selectors
# ... generate/edit the config ...
stepshots record --dry-run --json        # validate config structure
stepshots record -t tutorial-key --json  # record to output/<key>.stepshot
stepshots upload output/tutorial-key.stepshot  # create demo, prints the demo URL
```

Uploaded demos start private. The user publishes, edits overlays, and gets embed code
(iframe / JS snippet / React SDK / web component) from the dashboard.

To update an existing demo in place — keeping its URL, embeds, and analytics — re-record
and upload with `--demo-id`:

```bash
stepshots upload output/tutorial-key.stepshot --demo-id <existing-id>
```

## Authentication

- `stepshots login` — opens the browser, completes OAuth, stores an API token locally.
  Recommended for interactive use.
- `stepshots whoami` — verifies the stored (or passed) token and shows the account.
  Run this to diagnose auth errors before retrying an upload.
- `stepshots logout` — removes stored credentials.

Token precedence for `upload`: `--token` flag / `STEPSHOTS_TOKEN` env var, then the
stored login token. In CI, set `STEPSHOTS_TOKEN` as a secret instead of logging in.
`--server` / `STEPSHOTS_SERVER` overrides the server URL (defaults to `https://stepshots.com`).

## Product Model

The right mental model is:

- One meaningful action should usually become one demo step.
- A click step should show the scene where the user clicks.
- The resulting page or state should be a later step only if it matters.
- If you need a “resulting scene” without another action, use an explicit capture step in the extension or a `wait` step in CLI configs.

Do not optimize for showing everything that happened. Optimize for a short, intentional clickthrough.

## Recommended Flow Shape

Default to this structure:

1. Optional intro step:
   - `wait` on a stable selector with a short delay
   - use this only when a clean opening scene matters
2. Action step:
   - `click`, `type`, `select`, or `key`
3. Result step:
   - another meaningful action, or
   - `wait` if the new state itself should be shown

Avoid flows that are mostly scrolling or passive navigation.

## Recording for a Guided Tour

Stepshots produces two distinct assets, and the recording should know which one it's for:

- A **demo** is a screenshot artifact: sell voice, populated account, setup steps
  (navigate/wait/scroll) are legitimate content.
- A **guided tour** is a live overlay on the user's own app: instruct voice, and only
  interactive steps with a callout become tour steps. Its source of truth is a
  `tours/<key>.tour.json` file versioned in the app repo — the recording is only the
  scaffold that contributes selectors and fallback anchors once.

When the user wants a tour, set `"target": "tour"` on the tutorial and adjust how you
build the config:

1. **Voice**: callouts are instructions to the end user ("Click **New project** to
   create your first workspace"), not marketing copy.
2. **Starting state**: record on a fresh/empty account — the tour plays during
   activation, so the flow must work from the empty state, not a populated one.
3. **Coverage**: every `click`/`type`/`select` step needs a highlight callout, or it is
   silently dropped from the tour. `record` warns about violations. Navigations and
   waits never appear in tours — they only stage the recording.
4. **After recording**: `record` scaffolds `tours/<key>.tour.json` if it doesn't exist
   (an existing file is never overwritten — it's hand-edited source). Then:
   - `stepshots tour validate` — strict parse + lints, CI exit codes.
   - `stepshots tour check --url <staging>` — headless replay against a live deploy;
     `ok` = selector matched, `drift` = fallback anchor matched, `fail` = neither.
     `--update-fallbacks` refreshes text/aria anchors from the live DOM.
   - `stepshots tour build -o public/tours.js` — registry script for script-tag
     installs; bundler users import the `.tour.json` directly.
5. **Editing later**: edit the tour file, not the recording. Re-scaffold only when the
   flow itself changed: `stepshots tour init <key> --from <bundle> --force`.

For steps with `advance: input`/`change`, add a `check.value` so `tour check` can
type/select something realistic during replay. Use `check.path` when a step can't be
reached through the previous step's advance alone (e.g. after a full-page redirect).

## Preferred Actions

These are the primary actions to recommend:

| Action | Required Fields | Use For |
|---|---|---|
| `click` | `selector` | Buttons, links, tabs, toggles |
| `type` | `selector`, `text` | Text inputs |
| `select` | `selector`, `value` | Dropdowns |
| `key` | `key` | Keyboard shortcuts, Enter, Escape |
| `wait` | `selector` or `delay` | Stable opening states or result-only scenes |
| `navigate` | `url` | Explicit page jumps in CLI-authored configs |

These are supported but not recommended for the main product path:

| Action | Status | Guidance |
|---|---|---|
| `scroll` | legacy/edge case | Use only when scroll itself matters |
| `scroll-to` | edge case | Prefer direct state capture instead |
| `hover` | edge case | Do not rely on it for core screenshot demos |

## Current Extension Behavior

The extension now behaves like this:

- It records explicit actions only.
- It does not auto-create a separate navigation step after a click.
- A clicked target gets an automatic highlight.
- `Capture Screen` or `Alt+Shift+S` creates a screenshot-only step.
- If you click a nav link and want to also show the destination page, add a capture after navigation completes.

When describing extension workflows to users, explain it in exactly those terms.

## CLI Config Structure

```json
{
  "baseUrl": "https://example.com",
  "format": "desktop",
  "defaultDelay": 500,
  "tutorials": {
    "getting-started": {
      "url": "/",
      "title": "Getting Started",
      "description": "Short clickthrough of the core flow",
      "steps": []
    }
  }
}
```

### Viewport Formats

Prefer a named `format` over raw pixel dimensions:

| Format | Dimensions | Use For |
|---|---|---|
| `desktop-hd` | 1920×1080 | Hero demos, landing pages |
| `desktop` | 1280×800 | Default docs/README demos |
| `tablet-landscape` | 1024×768 | Tablet UIs |
| `tablet-portrait` | 768×1024 | Tablet UIs |
| `mobile` | 390×844 | Mobile flows |
| `mobile-landscape` | 844×390 | Mobile landscape |
| `square` | 1080×1080 | Social embeds |

A raw `"viewport": { "width": ..., "height": ... }` is still supported for custom sizes;
when both are present, `format` wins.

### Environment Variables in Configs

All string values support `${VAR}` interpolation, resolved at load time:

```json
{ "action": "type", "selector": "#password", "text": "${DEMO_PASSWORD}" }
```

Use this for credentials and environment-specific URLs so secrets never land in the
repo — and blur the affected fields (see Blur Regions) since typed values appear in
screenshots.

## Step Guidance

### `click`

Use for the primary progression in most demos.

```json
{
  "action": "click",
  "name": "Open pricing",
  "selector": "nav a[href='/pricing']",
  "highlights": [{
    "callout": "Open the pricing page",
    "position": "bottom",
    "showBorder": true
  }]
}
```

### `wait`

Use when the resulting scene matters but there is no new user action yet.

```json
{
  "action": "wait",
  "name": "View pricing page",
  "selector": "main h1",
  "delay": 1200,
  "highlights": [{
    "callout": "Plans are shown here",
    "position": "bottom",
    "showBorder": true
  }]
}
```

### `type`

```json
{
  "action": "type",
  "name": "Enter email",
  "selector": "#email",
  "text": "demo@example.com",
  "highlights": [{
    "callout": "Enter your work email",
    "position": "right",
    "showBorder": true
  }]
}
```

### `navigate`

Use only when you intentionally want a direct page jump in a CLI-authored flow. Do not use it as a replacement for every link click by default.

```json
{
  "action": "navigate",
  "name": "Go to docs",
  "url": "/docs"
}
```

## Overlays

For launch-quality screenshot demos, prefer simple overlays:

- one highlight per step
- short callout text
- occasional popup when more context is needed
- blur sensitive data

### Highlights

```json
{
  "highlights": [{
    "callout": "Start here",
    "position": "bottom",
    "showBorder": true,
    "arrow": false
  }]
}
```

Important:
- treat one primary highlight per step as the default
- keep callouts short
- the highlight should explain what the viewer should notice, not restate the selector

### Blur Regions

```json
"blurRegions": [{ "selector": ".api-key" }]
```

### Popups

Use only when a short highlight is not enough.

```json
"popups": [{
  "selector": ".analytics-widget",
  "title": "Analytics",
  "body": "Track views and completion here",
  "width": 280
}]
```

## Best Practices

1. Keep demos short. Target 4 to 8 steps.
2. Use one primary action per step.
3. Name every step with user intent, not implementation detail.
4. Prefer clickthrough scenes over scroll-heavy storytelling.
5. Use `wait` to show result states instead of inventing motion.
6. Prefer stable selectors (see Selector Discovery below).
7. Scope selectors when there are repeated elements.
8. Keep callout text compact.
9. Blur anything sensitive.
10. Record cleanly first; polish in the dashboard editor after upload.

## How You Should Build Configs

When generating configs:

- read an existing `stepshots.config.json` first if it exists
- preserve existing tutorial keys unless the user wants a new one
- avoid rewriting unrelated tutorials
- prefer adding a new tutorial entry rather than replacing the file wholesale
- keep JSON formatting clean and minimal

If the user gives a vague flow, inspect the page first.

## Selector Discovery

Use:

```bash
stepshots inspect https://example.com
```

Prefer selectors in this order:

1. `#id`
2. `[data-testid]`, `[data-cy]`, `[data-test]`
3. `[aria-label]`
4. semantic selectors like `button[type='submit']`
5. scoped class selectors

Avoid:

- generated IDs
- brittle `nth-child` chains unless there is no better option
- selectors that match multiple similar elements without context

## Preview and Validation Workflow

Recommended sequence:

```bash
stepshots preview tutorial-key
stepshots record --dry-run
stepshots record -t tutorial-key
```

Use:
- `preview` to verify selector accuracy and flow logic
- `record --dry-run` to validate config structure
- `record` for final capture

## Keeping Demos Fresh

Freshness is a two-part loop: **verify detects drift, then a human or agent repairs it.**
Never promise fully automatic re-recording — detection is reliable, blind repair is not.

### Detect drift with `verify`

```bash
stepshots verify                         # replay all tutorials headless, no bundle written
stepshots verify my-tutorial --fail-on warn   # one tutorial, annotation drift also fails
stepshots verify --json                  # machine-readable drift report for agents/CI
```

Exit code 0 means every checked tutorial still replays; 1 means drift was found (or the
config was invalid). A step that fails gets a debug screenshot (`output/<key>.failed-step-<n>.png`,
override the directory with `--save-failures`) and a repair hint in the report. Annotation
drift (a highlight/blur/arrow anchor that vanished or moved off-screen) is warn-level by
default: the demo still records, but the annotation would be dropped.

### Verify on a schedule in CI

Use the composite GitHub Action with `command: verify` to catch stale demos before users do:

```yaml
on:
  schedule:
    - cron: "0 6 * * 1"   # weekly
jobs:
  demo-freshness:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: hauju/stepshots@main
        with:
          command: verify
          config: demo/stepshots.config.json
```

The step fails when drift is found, writes a per-tutorial freshness summary to the job
summary, and saves `output/verify-report.json` for tooling. Authenticated flows need a
`profile-dir`-style setup or a staging environment reachable from CI.

### Re-record and update in place

To re-record and update a demo whenever the product changes, use the same action in
record mode with a repo secret and the existing demo's id:

```yaml
- uses: hauju/stepshots@main
  with:
    config: demo/stepshots.config.json
    tutorials: landing-page
    upload: "true"
    token: ${{ secrets.STEPSHOTS_API_KEY }}
    demo-id: ${{ vars.HERO_DEMO_ID }}
```

Because `demo-id` replaces the existing demo, every embed and share link stays valid
while the screenshots update.

## Dashboard Export Guidance

The dashboard can export a saved screenshot demo as `stepshots.config.json` for CLI recording.

Use this when:
- the user already created the flow in the extension
- they want a repo-friendly config

Be explicit about one limitation:
- older demos may not export fully if they were created before recording origin metadata was stored

## Example Config

```json
{
  "baseUrl": "https://app.example.com",
  "format": "desktop",
  "defaultDelay": 500,
  "tutorials": {
    "signup-flow": {
      "url": "/",
      "title": "Sign Up for Example App",
      "description": "Create your account in under a minute",
      "steps": [
        {
          "action": "wait",
          "name": "View homepage",
          "selector": "main",
          "delay": 1200,
          "highlights": [{
            "callout": "Start on the homepage",
            "position": "bottom",
            "showBorder": false
          }]
        },
        {
          "action": "click",
          "name": "Click Get Started",
          "selector": "[data-testid='get-started-btn']",
          "highlights": [{
            "callout": "Start the signup flow",
            "position": "bottom",
            "showBorder": true
          }]
        },
        {
          "action": "wait",
          "name": "View signup form",
          "selector": "form#signup",
          "delay": 1200,
          "highlights": [{
            "callout": "The signup form opens here",
            "position": "right",
            "showBorder": true
          }]
        },
        {
          "action": "type",
          "name": "Enter email",
          "selector": "#email",
          "text": "demo@example.com",
          "highlights": [{
            "callout": "Enter your email",
            "position": "right",
            "showBorder": true
          }]
        },
        {
          "action": "click",
          "name": "Submit form",
          "selector": "button[type='submit']",
          "delay": 1500,
          "highlights": [{
            "callout": "Submit to continue",
            "position": "top",
            "showBorder": true
          }]
        },
        {
          "action": "wait",
          "name": "View dashboard",
          "selector": ".dashboard",
          "delay": 1500,
          "highlights": [{
            "callout": "You land on the dashboard",
            "position": "bottom",
            "showBorder": false
          }]
        }
      ]
    }
  }
}
```

## AI Agent Integration

### Structured JSON Output

Use `--json` for machine-parseable output from `inspect` and `record`:

```bash
stepshots inspect https://example.com --json    # Discover selectors
stepshots list --json                           # Tutorials defined in the config
stepshots record --dry-run --json               # Validate config
stepshots record tutorial-key --json            # Record with structured result
stepshots verify --json                         # Drift report: do demos still replay?
stepshots upload output/key.stepshot --json     # Upload, returns demo_id + view_url
stepshots doctor --json                         # Environment checks (browser/config/server/auth)
stepshots schema                                # JSON Schema for stepshots.config.json
```

`stepshots schema` prints the full JSON Schema for the config file — use it as the
authoritative field reference when generating configs. Configs written by
`stepshots init` carry a `$schema` key so editors validate them too.

In `--json` mode, the only stdout output is a single JSON object. Human-readable messages are suppressed. Progress bars and warnings go to stderr. `upload --json` returns `{"demos": [{"demo_id", "view_url", "replaced"}]}`; errors from any command are emitted as JSON when `--json` is set.

### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Config error (bad JSON, missing file, validation) |
| 2 | Browser error (Chrome not found, crash) |
| 3 | Action error (selector timeout, click failure) |
| 4 | Bundle error (ZIP/manifest issue) |
| 5 | Upload / auth error |

`verify` exits 1 both for drift and for config errors — branch on the JSON instead of
the exit code: drift reports have a `summary` object, config errors have a top-level
`error` object.

### Recommended AI Agent Workflow

1. `stepshots inspect <url> --json` — discover page selectors
2. Generate `stepshots.config.json` from natural language + discovered selectors
3. `stepshots record --dry-run --json` — validate config structure
4. If validation fails, fix the config based on the error JSON
5. `stepshots record -t <key> --json` — record the demo
6. Parse JSON result — if a step failed with a selector error:
   - Read the debug screenshot (`output/<key>.failed-step-<n>.png`) to see the page state
   - Run `stepshots inspect <url> --json` on the failure-time URL to find the correct selector
   - Update the config and retry
7. `stepshots upload output/<key>.stepshot --json` — publish, parse `demo_id` and
   `view_url` from the result (exit code 5 means auth: run `stepshots doctor --json`
   to see what's wrong, then `stepshots login` or set `STEPSHOTS_TOKEN`)

If the environment itself seems broken (browser missing, server unreachable), run
`stepshots doctor --json` — it checks browser, config, server, and auth in one call.

### Demo Drift Repair Workflow

When asked to check or fix stale demos, run `stepshots verify --json` and repair from
the report. Each failed step carries the repair contract: `reason`, `page_url`,
`screenshot`, and a `hint` naming the next command.

1. `stepshots verify --json` — replay every tutorial, collect the drift report
2. For each `"status": "fail"` step, branch on `reason`:
   - `orphaned` — the selector matched nothing. Read the `screenshot` to see the page,
     run `stepshots inspect <page_url> --json`, pick a stable replacement selector,
     update that step in the config
   - `navigation_failed` — the URL no longer loads. Fix the step's `url` (or the
     tutorial `url` / `baseUrl` when the tutorial-level `error` is set)
   - `action_failed` — the element exists but the action broke (e.g. a `select` value
     that's gone). Read the screenshot and adjust the step
3. For `annotation_warnings` (warn-level), fix the annotation's selector or drop the
   annotation — the flow itself still works
4. Re-run `stepshots verify -t <key> --json` until it exits 0
5. Re-record with `stepshots record -t <key> --json`, then republish with
   `stepshots upload output/<key>.stepshot --demo-id <existing-id>` so embeds and
   share links stay valid

Show the user what changed in the config before uploading — repair is assisted, not
silent.

### JSON Output Shapes

**record** (success):
```json
{
  "success": true,
  "command": "record",
  "tutorials": [{
    "key": "signup-flow",
    "title": "Sign Up",
    "output": "output/signup-flow.stepshot",
    "steps_total": 6,
    "steps_completed": 6,
    "steps": [
      { "index": 0, "name": "View homepage", "action": "wait", "status": "ok" },
      { "index": 1, "name": "Click Get Started", "action": "click", "selector": "[data-testid='cta']", "status": "ok" }
    ]
  }]
}
```

**record** (failure): a failed step aborts its tutorial (no bundle is written), but the
remaining tutorials still record. The exit code is non-zero and the output contains
per-step status plus the first failure with its step index and tutorial key. A debug
screenshot of the page at failure time is saved as `output/<key>.failed-step-<n>.png`
(1-based step number) — read it to see what the page actually looked like.

```json
{
  "success": false,
  "command": "record",
  "tutorials": [{
    "key": "signup-flow",
    "title": "Sign Up",
    "steps_total": 6,
    "steps_completed": 3,
    "steps": [
      { "index": 2, "action": "type", "selector": "input[name='email']", "status": "ok" },
      { "index": 3, "action": "click", "selector": "#btn", "status": "failed", "error": "Action error: Timed out waiting for selector '#btn' before 'click' action" }
    ]
  }],
  "error": { "category": "action", "message": "Action error: Timed out waiting for selector '#btn' before 'click' action", "step_index": 3, "tutorial": "signup-flow" }
}
```

**verify**: replays steps without writing bundles. Tutorial `status` is `ok`, `warn`
(annotation drift only), or `fail`; steps after a failure are `skipped` since the page
state is undefined. A start page that no longer loads sets a tutorial-level `error`
instead of a step failure.

```json
{
  "success": false,
  "command": "verify",
  "config": "stepshots.config.json",
  "summary": { "tutorials": 2, "ok": 1, "warn": 0, "fail": 1, "steps_checked": 3 },
  "tutorials": [
    { "key": "landing-page", "title": "Landing Page", "status": "ok",
      "steps": [ { "index": 0, "action": "click", "selector": "#cta", "status": "ok" } ] },
    { "key": "signup-flow", "title": "Sign Up", "status": "fail",
      "steps": [
        { "index": 0, "action": "click", "selector": "#open", "status": "ok" },
        { "index": 1, "action": "click", "selector": "#old-btn", "status": "fail",
          "reason": "orphaned",
          "message": "Action error: Timed out waiting for selector '#old-btn' before 'click' action",
          "page_url": "https://app.example.com/signup",
          "screenshot": "output/signup-flow.failed-step-2.png",
          "hint": "Run `stepshots inspect https://app.example.com/signup` to find a replacement selector, then update this step in stepshots.config.json." },
        { "index": 2, "action": "type", "selector": "#email", "status": "skipped" }
      ],
      "annotation_warnings": [
        { "step": 0, "kind": "highlight", "selector": ".banner", "reason": "off_screen" }
      ] }
  ]
}
```

**inspect**:
```json
{
  "url": "https://example.com",
  "elements": [
    { "index": 1, "tag": "a", "selector": "nav a[href='/pricing']", "text": "Pricing", "href": "/pricing", "bounds": { "x": 800, "y": 12, "width": 60, "height": 20 } }
  ]
}
```

## Summary

This skill should push AI agents toward:

- screenshot demos over live replay
- short clickthroughs over long motion-heavy recordings
- explicit scenes over inferred navigation
- clean config generation
- CLI preview/record workflows
- uploading and publishing, not stopping at a local `.stepshot` file
- `verify` for drift detection, with assisted (never silent) repair
- dashboard export when a saved demo already exists
- using `--json` for programmatic feedback loops
