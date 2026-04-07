---
name: stepshots-cli-record
description: |
  Create screenshot-based product demos using Stepshots from natural-language flow descriptions.
  Use when a user wants to record a clickthrough demo, generate or edit
  stepshots.config.json, or turn an existing saved Stepshots demo into a CLI recording config.
  Works for both CLI-first and AI-assisted workflows.
author: Hauke Jung
version: 2.0.0
---

# Stepshots CLI Screenshot Demo Skill

Use this skill when the user wants a screenshot clickthrough demo, not a live HTML replay.

This skill is adapted to the current product model:
- screenshot demos are the primary path
- click steps are target-first
- the extension records explicit actions only
- `Capture Screen` creates a screenshot-only step
- old auto-recorded navigation assumptions are no longer the recommended model

## What Codex Should Do

When using this skill, Codex should:

1. Understand the flow the user wants to show.
2. Prefer a clean clickthrough sequence over trying to show every intermediate UI motion.
3. Generate or update `stepshots.config.json`.
4. Prefer `click`, `type`, `select`, `key`, `wait`, and explicit scene captures.
5. Use the CLI to preview, validate, and record.
6. If the user already has a saved demo, export or reconstruct a CLI config from that demo when possible.

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
  "viewport": { "width": 1280, "height": 800 },
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
6. Prefer stable selectors: `#id`, `[data-testid]`, `[aria-label]`, then semantic selectors.
7. Scope selectors when there are repeated elements.
8. Keep callout text compact.
9. Blur anything sensitive.
10. Record cleanly first; polish in the dashboard editor after upload.

## How Codex Should Build Configs

When generating configs:

- read an existing `stepshots.config.json` first if it exists
- preserve existing tutorial keys unless the user wants a new one
- avoid rewriting unrelated tutorials
- prefer adding a new tutorial entry rather than replacing the file wholesale
- keep JSON formatting clean and minimal

If the user gives a vague flow, Codex should inspect the page first.

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
  "viewport": { "width": 1280, "height": 800 },
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

Use `--json` for machine-parseable output from any command:

```bash
stepshots inspect https://example.com --json    # Discover selectors
stepshots record --dry-run --json               # Validate config
stepshots record -t tutorial-key --json         # Record with structured result
```

In `--json` mode, the only stdout output is a single JSON object. Human-readable messages are suppressed. Progress bars and warnings go to stderr.

### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Config error (bad JSON, missing file, validation) |
| 2 | Browser error (Chrome not found, crash) |
| 3 | Action error (selector timeout, click failure) |
| 4 | Bundle error (ZIP/manifest issue) |
| 5 | Upload / auth error |

### Recommended AI Agent Workflow

1. `stepshots inspect <url> --json` — discover page selectors
2. Generate `stepshots.config.json` from natural language + discovered selectors
3. `stepshots record --dry-run --json` — validate config structure
4. If validation fails, fix the config based on the error JSON
5. `stepshots record -t <key> --json` — record the demo
6. Parse JSON result — if a step failed with a selector error:
   - Run `stepshots inspect <url> --json` to find the correct selector
   - Update the config and retry
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

**record** (failure):
```json
{
  "success": false,
  "error": { "category": "action", "message": "Timed out waiting for selector '#btn'" }
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
- dashboard export when a saved demo already exists
- using `--json` for programmatic feedback loops
