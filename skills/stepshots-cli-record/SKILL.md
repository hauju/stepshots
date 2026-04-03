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

If the user gives vague instructions like "demo the signup flow", use the **inspect** command to discover the page structure and selectors:

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
| `scroll` | `scrollX`, `scrollY` | Scroll the page or element (relative, uses `scrollBy`) |
| `scroll-to` | `selector` | Scroll an element into view (`scrollIntoView` with `block:'center'`) |
| `hover` | `selector` | Focus/hover an element |
| `navigate` | `url` | Go to a URL (relative or absolute) |
| `wait` | `selector` or `delay` | Wait for element to appear or delay in ms |
| `select` | `selector`, `value` | Set a dropdown value |

#### Annotations (Overlays)

Add these to any step to annotate the screenshot taken AFTER the step's action executes. Overlay selectors are resolved against the current viewport — elements must be visible on screen at that scroll/navigation position. Off-screen overlays are automatically skipped with a warning during recording.

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
The highlight targets the step's own `selector` by default. Override with `highlightSelector` on the step to highlight a different element than the action target. `position` can be `top`, `bottom`, `left`, or `right`.

**IMPORTANT: Only one highlight per step.** The CLI only resolves `highlights[0]` — additional entries are ignored. Highlights do NOT have their own `selector` field; they always use the step's `selector` (or `highlightSelector`). To annotate multiple elements in one step, use the highlight for the primary element and hotspots or popups for secondary elements (these DO have their own `selector` fields).

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

12. **Overlays must target visible elements** — Highlights, hotspots, and other overlays are only recorded when their target element is visible in the viewport at that step. If a highlight targets an element that has scrolled off-screen, it will be skipped with a warning. Make sure scroll steps bring target elements into view before annotating them.

13. **One highlight per scroll section** — When scrolling through a long page, each scroll step should highlight elements visible at that scroll position. Don't add a highlight targeting an element from a previous scroll position — it won't be visible and will be skipped.

14. **Callout position matters** — Choose `position` (`top`, `bottom`, `left`, `right`) based on where there's space around the target element. Elements near edges may have their callout auto-flipped to the opposite side. Avoid `left` position for elements near the left edge.

15. **Keep callout text short (5-8 words)** — Long callouts get squeezed into narrow columns, especially near viewport edges. "Search docs with ⌘K" beats "Instant search across all docs — find any topic in seconds with ⌘K". If you need more text, use a `popup` instead (it has a fixed `width` parameter).

16. **`scroll-to` skips the delay** — For scroll actions, the step's `delay` is skipped because the CLI captures transition frames (~600ms) instead. If the scroll is long and overlays resolve off-screen, split into two steps: a bare `scroll-to` step (no overlays), then a `wait` step with `delay` and the overlays.

17. **Prefer `scroll-to` over `scroll`** — `scroll` uses relative `scrollBy` with pixel offsets that break when page layout changes. `scroll-to` uses `scrollIntoView` on a selector, which adapts to any layout. Use `scroll` only when you need precise pixel control.

18. **Prefer `navigate` over `click` for nav links** — Clicking nav links can fail if the element is obscured after scrolling. `navigate` with a relative URL is more reliable for page transitions.

19. **Use section IDs for scroll targets** — When scrolling through a long page, target stable `#id` selectors (e.g., `#features`, `#pricing`) rather than class-based selectors that may break with Tailwind/CSS changes.

20. **Record clean, polish in dashboard** — For marketing-quality demos, record screenshots with minimal CLI overlays, then upload and use the visual overlay editor in the dashboard to place annotations precisely. The CLI is best for capturing the flow; the editor is best for making it look polished.

21. **Add a `wait` step at the start** — Before any actions, add a `wait` step with a stable selector (e.g. `"selector": "body"` or a hero element) and a `delay` of 1000-1500ms. This ensures the page is fully rendered before the first screenshot. Dynamic pages (SPAs, lazy-loaded content) need this especially.

22. **Preview → dry-run → record** — Follow this workflow: `stepshots preview` (visible browser, verify flow works) → `stepshots record --dry-run` (validate config without Chrome) → `stepshots record` (final recording). Don't skip straight to record.

23. **Scope selectors with context** — When a page has multiple similar elements (e.g. several "Submit" buttons), scope selectors to their container: use `#signup-form button[type="submit"]` instead of just `button[type="submit"]`. This prevents matching the wrong element.

24. **Use `rerecord` when UI changes** — When the target site updates its design, run `stepshots rerecord output/tutorial.stepshot` instead of re-recording from scratch. This replays all actions on the current version while preserving your overlay annotations, saving manual annotation work.

25. **Add delay for page transitions** — Steps that trigger navigation (click on a link, form submit) need `"delay": 1500` or higher to let the new page fully load before the screenshot is captured. Default 500ms is too fast for most page transitions.

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
2. **Prefer stability**: `#id` > `[data-testid]` > `[aria-label]` > `.class` > CSS path
3. **Test selectors** in browser devtools: `document.querySelector('your-selector')`
4. **Avoid**: Selectors with dynamic IDs, deeply nested paths, or nth-child chains
