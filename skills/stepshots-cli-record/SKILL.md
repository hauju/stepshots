---
name: stepshots-cli-record
description: |
  Create interactive product demos using the Stepshots CLI from simple natural language instructions.
  Generates stepshots.config.json with steps, annotations, and overlays, then records and uploads.
  Use when: (1) User wants to create a product demo or tutorial for a website,
  (2) User says "record a demo", "create a stepshot", "demo this flow",
  (3) User describes a click-through flow they want captured as an interactive demo,
  (4) User wants to document a UI workflow with highlights, callouts, and annotations.
  Triggers: "stepshot", "record demo", "create demo", "product demo", "interactive demo",
  "demo this", "capture flow", "stepshots record", "tutorial demo".
author: Hauke Jung
version: 1.0.0
---

# Stepshots CLI Demo Creator

Create interactive product demos from simple instructions using the Stepshots CLI.

## Prerequisites

The Stepshots CLI must be installed:
```bash
cargo install stepshots-cli
```

Chrome or Chromium must be available. Set `CHROME_PATH` env var if not in the default location.

## Workflow

### Phase 1: Understand the Flow

Ask the user what flow they want to capture. You need:
- **Target URL** — The starting page
- **Steps** — What to click, type, navigate to (in order)
- **What to highlight** — Which elements deserve callouts or annotations

If the user gives vague instructions like "demo the signup flow", use the **inspect** command or Playwright MCP tools to discover the page structure and selectors:

```bash
stepshots inspect https://example.com
```

This launches an interactive REPL showing all clickable elements with their CSS selectors. Use the element numbers to identify correct selectors.

### Phase 2: Generate the Config

Create `stepshots.config.json` in the project directory. If one exists, read it first and add a new tutorial entry.

#### Config Structure

```json
{
  "baseUrl": "https://example.com",
  "viewport": { "width": 1280, "height": 800 },
  "defaultDelay": 500,
  "tutorials": {
    "tutorial-key": {
      "url": "/starting-path",
      "title": "Human-Readable Title",
      "description": "What this demo shows",
      "steps": []
    }
  }
}
```

#### Step Names

Every step supports an optional `name` field — a short, human-readable label describing what the step does. **Always set `name` on every step.** These names appear in analytics charts (completion funnel, time-per-step) so users can see exactly where viewers drop off.

Good names are concise and describe the user intent, not the technical action:
- "Click Get Started" (not "click button.cta")
- "Enter email address" (not "type into #email")
- "View dashboard" (not "wait for .dashboard")

```json
{
  "action": "click",
  "name": "Click Get Started",
  "selector": "[data-testid='get-started-btn']"
}
```

#### Step Actions

Each step has an `action` and typically a `selector`. Available actions:

| Action | Required Fields | Description |
|--------|----------------|-------------|
| `click` | `selector` | Click an element |
| `type` | `selector`, `text` | Clear field and type text |
| `key` | `key` | Press a key (Enter, Escape, Tab, etc.) |
| `scroll` | `scrollX`, `scrollY` | Scroll the page or element |
| `hover` | `selector` | Focus/hover an element |
| `navigate` | `url` | Go to a URL (relative or absolute) |
| `wait` | `selector` or `delay` | Wait for element to appear or delay in ms |
| `select` | `selector`, `value` | Set a dropdown value |

#### Annotations (Overlays)

Add these to any step to annotate the screenshot taken BEFORE the action executes:

**Highlights** — Draw attention to an element with optional callout text:
```json
{
  "action": "click",
  "selector": "#signup-btn",
  "highlights": [{
    "callout": "Click here to sign up",
    "position": "bottom",
    "showBorder": true,
    "arrow": true
  }]
}
```
The highlight targets the step's own `selector` by default. `position` can be `top`, `bottom`, `left`, or `right`.

**Blur Regions** — Redact sensitive content:
```json
"blurRegions": [{ "selector": ".credit-card-number" }]
```

**Hotspots** — Pulsing indicators on elements:
```json
"hotspots": [{
  "selector": ".important-feature",
  "callout": "New feature!",
  "position": "top",
  "size": 20
}]
```

**Popups** — Rich info tooltips:
```json
"popups": [{
  "selector": ".dashboard-widget",
  "title": "Analytics Dashboard",
  "body": "View real-time metrics here",
  "width": 300
}]
```

**Arrows** — Connect two elements visually:
```json
"arrows": [{
  "fromSelector": ".step-1",
  "toSelector": ".step-2",
  "color": "#FF0000",
  "strokeWidth": 2
}]
```

### Phase 3: Best Practices for Great Demos

Follow these rules when generating configs:

1. **5-10 steps max** — Keep demos focused. Users drop off after ~10 steps.

2. **Name every step** — Always set `"name"` on every step. Names appear in analytics charts so demo owners can understand viewer behavior. Use concise, intent-driven labels like "Click Sign Up" or "Enter email".

3. **Annotate every step** — Every step should have at least one highlight with a callout explaining what's happening and why.

4. **Lead with context** — Step 0 (the initial screenshot) should have a highlight or popup explaining what the user is looking at.

5. **Use delays wisely** — Add `"delay": 1000` for steps that trigger animations or loading states. Default 500ms works for most clicks.

6. **Blur sensitive data** — Always add `blurRegions` for email addresses, personal data, API keys, or anything that shouldn't be in a public demo.

7. **Pick stable selectors** — Prefer `#id`, `[data-testid="..."]`, or `[aria-label="..."]` over fragile class-based selectors. Use `stepshots inspect` to find good ones.

8. **Test with preview first** — Before recording, run preview to verify the flow works:
   ```bash
   stepshots preview tutorial-key
   ```

9. **Use `--dry-run` to validate** — Check your config without launching Chrome:
   ```bash
   stepshots record --dry-run
   ```

10. **One tutorial per feature** — Don't cram multiple features into one demo. Create separate tutorial keys.

11. **Descriptive tutorial keys** — Use kebab-case keys that describe the flow: `signup-flow`, `create-first-project`, `invite-team-member`.

### Phase 4: Record

```bash
# Record a specific tutorial
stepshots record -t tutorial-key

# Record all tutorials
stepshots record

# Record to a specific directory
stepshots record -t tutorial-key -o ./demos
```

Output: `output/tutorial-key.stepshot` (ZIP bundle with manifest + PNG screenshots).

### Phase 5: Upload (Optional)

If the user wants to publish:

```bash
# Set auth token
export STEPSHOTS_TOKEN="your-api-token"

# Upload
stepshots upload output/tutorial-key.stepshot

# Upload with custom title
stepshots upload output/tutorial-key.stepshot --title "Getting Started Guide"
```

The upload returns a shareable URL.

### Phase 6: Re-record (When UI Changes)

To update screenshots without rewriting the config:

```bash
stepshots rerecord output/tutorial-key.stepshot
```

This replays all actions on the current version of the site and captures fresh screenshots while preserving annotations.

## Example: Complete Config

Here's a well-structured demo config for a SaaS signup flow:

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
          "action": "click",
          "name": "Click Get Started",
          "selector": "[data-testid='get-started-btn']",
          "highlights": [{
            "callout": "Start by clicking Get Started on the homepage",
            "position": "bottom",
            "showBorder": true
          }]
        },
        {
          "action": "type",
          "name": "Enter email address",
          "selector": "#email",
          "text": "demo@example.com",
          "highlights": [{
            "callout": "Enter your email address",
            "position": "right",
            "showBorder": true
          }],
          "blurRegions": [{ "selector": ".social-login-section" }]
        },
        {
          "action": "type",
          "name": "Set password",
          "selector": "#password",
          "text": "SecurePass123!",
          "highlights": [{
            "callout": "Choose a strong password",
            "position": "right",
            "showBorder": true
          }]
        },
        {
          "action": "click",
          "name": "Submit sign-up form",
          "selector": "button[type='submit']",
          "delay": 1500,
          "highlights": [{
            "callout": "Submit to create your account",
            "position": "top",
            "showBorder": true,
            "arrow": true
          }]
        },
        {
          "action": "wait",
          "name": "View dashboard",
          "selector": ".dashboard",
          "delay": 2000,
          "highlights": [{
            "callout": "Welcome to your new dashboard!",
            "position": "bottom",
            "showBorder": false
          }],
          "popups": [{
            "selector": ".onboarding-wizard",
            "title": "Next Steps",
            "body": "The onboarding wizard will guide you through setup",
            "width": 280
          }]
        }
      ]
    }
  }
}
```

## Selector Discovery Tips

When the user doesn't provide selectors, discover them:

1. **Use `stepshots inspect`** for an interactive element browser
2. **Use Playwright MCP** (`browser_snapshot`) to get a DOM snapshot
3. **Prefer stability**: `#id` > `[data-testid]` > `[aria-label]` > `.class` > CSS path
4. **Test selectors** in browser devtools: `document.querySelector('your-selector')`
5. **Avoid**: Selectors with dynamic IDs, deeply nested paths, or nth-child chains
