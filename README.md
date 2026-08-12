# Stepshots

Open-source tools for recording interactive product demos.

This repo contains the **CLI**, **Chrome extension**, **React SDK**, and **live guided-tour player** for [Stepshots](https://stepshots.com) — capture step-by-step screenshots, bundle them into `.stepshot` files, embed them anywhere, and replay a recording as guided onboarding on your real app.

Learn more on the [CLI page](https://stepshots.com/cli) and the [live tours page](https://stepshots.com/tour), or read the full [documentation](https://stepshots.com/docs/getting-started/introduction).

## Installation

**Quick install** (macOS Apple Silicon, Linux x86_64, Linux aarch64):

```sh
curl -sSL https://raw.githubusercontent.com/hauju/stepshots/main/install.sh | sh
```

The script downloads the latest release binary from GitHub into `~/.local/bin` (override with `STEPSHOTS_INSTALL_DIR`). Pin a specific version with `STEPSHOTS_VERSION=v1.0.1`.

**Via Cargo** (any platform with a Rust toolchain):

```sh
cargo install stepshots-cli
```

Requires Chrome or Chromium installed on your system.

## Usage

### Initialize a config file

```sh
stepshots init
```

Creates a `stepshots.config.json` with a sample tutorial definition. The file carries a `$schema` reference, so editors with JSON Schema support (VS Code, JetBrains, Zed, …) autocomplete and validate it as you type. Print the schema itself with `stepshots schema`.

### Record tutorials

```sh
# See what's defined in the config
stepshots list

# Record all tutorials defined in the config
stepshots record

# Record a specific tutorial
stepshots record my-tutorial

# Preview in a visible browser
stepshots preview my-tutorial
```

If a step fails (usually a selector that no longer matches), the CLI saves a screenshot of the page at failure time (`output/<key>.failed-step-<n>.png`), prints the page URL, and continues with the remaining tutorials. Use `stepshots inspect <url>` to find the right selector and `stepshots preview <key>` to watch the flow live.

### Verify demos are still up to date

```sh
# Replay all tutorials against the live app without writing bundles
stepshots verify

# Verify one tutorial, and treat annotation drift as a failure too
stepshots verify my-tutorial --fail-on warn
```

`verify` replays each tutorial headless and reports drift instead of recording: selectors that no longer match, start pages that fail to load, and annotations that lost their anchor. Exit code 0 means everything is fresh; 1 means drift was found. Add `--json` for a machine-readable report (for CI and AI agents) with a repair hint per failure; failure screenshots land in `output/` (change with `--save-failures`).

To check demos on a schedule in CI, use the GitHub Action with `command: verify`:

```yaml
- uses: hauju/stepshots@main
  with:
    command: verify
    config: demo/stepshots.config.json
```

The step fails when drift is found and writes a freshness summary to the job summary, plus `output/verify-report.json` for tooling.

### Guided tours

A guided tour is its own text-only asset — a `tours/<key>.tour.json` file you version in the app repo whose DOM it targets — played by [`@stepshots/tour`](packages/tour/) as a live overlay on your real app. A recording is the scaffold, not the source: it contributes the selectors and fallback anchors once, then the tour file is yours to edit.

```sh
# Scaffold a tour (from scratch, or projecting a recorded bundle)
stepshots tour init onboarding
stepshots tour init onboarding --from output/onboarding.stepshot

# Static validation: strict schema + lints, CI-friendly exit codes
stepshots tour validate

# Live validation: replay the tour headless against a deploy.
# ok = selector matched · drift = fallback anchor matched · fail = neither
stepshots tour check --url https://staging.example.com

# Refresh fallback anchors from the live DOM (selector hits only)
stepshots tour check --url https://staging.example.com --update-fallbacks

# Merge tour files into a window.__STEPSHOTS_TOURS script for script-tag installs
stepshots tour build -o public/tours.js

# Or let stepshots.com host them: upsert by key, get a one-line embed script
stepshots tour push
```

Tours localize like any other file in your repo. `tour init onboarding --locale de`
scaffolds `tours/onboarding.de.tour.json` from the base file — translate the
strings, leave the structure. `tour validate` fails CI when a translation falls
out of sync with its base (step count drift), and `tour build --locale de`
emits a registry that prefers `de` variants and falls back to the base per
tour. Run `tour check --url <localized-staging> --update-fallbacks` on a
variant to capture the localized text anchors. Hosted tours serve the default
locale; `push` skips variants.

`push` is one-way: your git files stay the source of truth, and pushing overwrites the hosted copy. Hosted tours get anonymous run analytics in the dashboard.

Tag a tutorial with `"target": "tour"` in `stepshots.config.json` and `record` warns about interactive steps without a callout (they'd be dropped from the tour) and scaffolds the tour file automatically. `verify` and `tour check` are siblings: one guards your demos, the other your tours.

To run the tour check in CI, use the GitHub Action with `command: tour-check`:

```yaml
- uses: hauju/stepshots@main
  with:
    command: tour-check
    url: https://staging.example.com
    fail-on: warn
```

The step fails when a tour breaks, writes a tour freshness summary to the job summary, and leaves `output/tour-check-report.json` for tooling.

Tour files get editor autocomplete and validation via their `$schema` entry (`schema/tour.schema.json`).

### Record logged-in flows

Recordings run in a fresh headless browser, so sites you're normally signed in to appear logged out. To record an authenticated flow, log in once inside a persistent browser profile, then point recordings at it:

```sh
# One-time: opens a visible browser — log in, then press Ctrl+C
stepshots browser https://github.com/login --profile-dir ~/.stepshots/profile

# Recordings (and preview/inspect) reuse the saved session
stepshots record --tutorial my-tutorial --profile-dir ~/.stepshots/profile
```

Set `STEPSHOTS_PROFILE_DIR` to avoid repeating the flag. Use a dedicated profile directory — never your regular Chrome profile.

#### Logged-in flows in CI

A browser profile can't travel to CI: it's bulky and binary, tied to a machine and a Chrome version, it holds live session tokens so it must never be committed, and the session inside it expires anyway. For CI, hand the session over as JSON instead:

```sh
stepshots verify --storage-state auth.json
stepshots record --tutorial my-tutorial --storage-state auth.json
```

The file is Playwright's `storageState` format, so if you already run browser tests you can produce one from your existing login setup:

```js
await context.storageState({ path: 'auth.json' });
```

Or write it by hand — every field but `name`/`value` is optional:

```json
{
  "cookies": [
    { "name": "session", "value": "…", "domain": "app.example.com", "path": "/" }
  ],
  "origins": [
    {
      "origin": "https://app.example.com",
      "localStorage": [{ "name": "token", "value": "…" }]
    }
  ]
}
```

Cookies are applied before the first navigation, and `localStorage` is restored by a script that runs before page scripts on every document — so the first page you land on is already authenticated.

**Generate it during the CI run and never commit it.** It is credentials in JSON form. `STEPSHOTS_STORAGE_STATE` sets the path if you'd rather not repeat the flag.

### Amend a recording (one-shot flows)

Some flows can't be re-recorded — a signup wizard you can only complete once, a destructive action, a third-party app. `stepshots patch` amends an existing `.stepshot` bundle with manually captured steps instead: it opens a visible browser locked to the bundle's viewport (so captures match pixel-for-pixel), you stage each page by hand and press Enter to capture, then Ctrl+C saves the bundle (a backup of the original is kept next to it).

```sh
# Append captured steps at the end
stepshots patch output/my-tutorial.stepshot

# Insert captured steps starting at position 3 (later steps shift back)
stepshots patch output/my-tutorial.stepshot --at 3

# Re-capture step 5's screenshot, keeping its metadata and overlays
stepshots patch output/my-tutorial.stepshot --replace 5

# Authenticated pages: reuse a logged-in profile (see above)
stepshots patch output/my-tutorial.stepshot --profile-dir ~/.stepshots/profile
```

By default the browser opens at the URL stored in the bundle; override it with `--url`. Afterwards, `stepshots upload <bundle> --demo-id <DEMO_ID>` replaces the hosted demo in place.

### Authenticate

Log in through your browser — this stores an API token locally at `~/.config/stepshots/tokens.json`:

```sh
stepshots login
```

Check which account you're logged in as:

```sh
stepshots whoami
```

For CI or scripts, set `STEPSHOTS_TOKEN` instead of logging in (generate one from [API keys](https://stepshots.com/docs/guides/api-keys)). Set `STEPSHOTS_SERVER` to point at a self-hosted instance.

### Upload to Stepshots

```sh
# Upload a recorded bundle (private draft)
stepshots upload output/my-tutorial.stepshot

# Publish it immediately (great for CI pipelines)
stepshots upload output/my-tutorial.stepshot --public

# Replace an existing demo
stepshots upload output/my-tutorial.stepshot --demo-id <DEMO_ID>

# Use a custom server
stepshots upload output/my-tutorial.stepshot --server https://your-instance.com
```

Uploads use your `stepshots login` session, or `STEPSHOTS_TOKEN` / `--token` when set. Override the server with `--server` or `STEPSHOTS_SERVER`. Add `--public` to make new demos publicly viewable right away (ignored with `--demo-id`).

### Check your setup

```sh
stepshots doctor
```

Verifies your browser (Chrome/Chromium + version), config file, server reachability, and login in one pass — run it first when something misbehaves, and include its output in bug reports.

### MCP server (for AI agents)

```sh
# Claude Code
claude mcp add stepshots -- stepshots mcp
```

`stepshots mcp` serves the recording workflow over the [Model Context Protocol](https://modelcontextprotocol.io) on stdio, so AI agents can drive it as tools: `get_schema` (write or validate a config), `list_tutorials`, `record`, `verify` (drift report with a repair hint per failure), and `upload` (publish, or update an existing demo in place via `demo_id`). Any MCP client works — configure the command `stepshots mcp` with no arguments:

```json
{
  "mcpServers": {
    "stepshots": { "command": "stepshots", "args": ["mcp"] }
  }
}
```

Run it from the project directory containing `stepshots.config.json` (or pass `--config`). Uploading needs a stored login (`stepshots login`) or `STEPSHOTS_TOKEN`.

### Shell completions

```sh
# fish
stepshots completions fish > ~/.config/fish/completions/stepshots.fish

# zsh / bash / powershell / elvish work the same way
stepshots completions zsh
```

### Upgrade

```sh
stepshots upgrade
```

Upgrades in place using however you installed — a fresh prebuilt binary for `install.sh` installs, or `cargo install` for Cargo installs. Add `--check` to only check for a newer version.

## Configuration

Tutorials are defined in `stepshots.config.json`. See `stepshots init` for an example, or the [configuration reference](https://stepshots.com/docs/cli/configuration) for every available option.

## Chrome Extension

The `extension/` directory contains the Stepshots Recorder — a Chrome extension that records interactions directly in the browser.

Install it from the [Chrome Web Store](https://chromewebstore.google.com/detail/stepshots-recorder/jgpahfgfkmojklbiphnfpklnpchkjhpd), or build it from source:

```sh
cd extension
bun install
bun run build
```

Then load the `extension/` folder as an unpacked extension in `chrome://extensions`.

## React SDK

```sh
bun add @stepshots/react
```

```tsx
import { StepshotsDemo } from "@stepshots/react";

<StepshotsDemo demoId="your-demo-id" />
```

See [`packages/react/`](packages/react/) or the [React SDK guide](https://stepshots.com/docs/guides/react-sdk) for full props documentation.

## Guided Tour Player

```sh
bun add @stepshots/tour
```

A framework-agnostic player that runs a guided tour on your real app — spotlighting the next element and advancing on the user's actual clicks and typing. Author tours as `*.tour.json` files (see [Guided tours](#guided-tours)), import them directly with a bundler or emit a registry script with `stepshots tour build`, or let [stepshots.com](https://stepshots.com) host tours for you with a single script tag. It also ships `createChecklist`, an onboarding checklist widget that bundles your activation tours into a persistent "Getting started · 2/5" launcher and checks items off as their tours complete.

See [`packages/tour/`](packages/tour/) or the [guided tours guide](https://stepshots.com/docs/guides/live-tours).

## Embed Examples

The `examples/` directory has ready-to-use HTML files showing how to embed Stepshots demos (see the [embedding guide](https://stepshots.com/docs/guides/embedding)):

- **`embed-js-snippet.html`** — Lightweight JS snippet integration
- **`embed-web-component.html`** — `<stepshots-demo>` web component
- **`embed-iframe.html`** — Simple iframe embed

## Project Structure

- **`crates/cli/`** — CLI binary (`stepshots-cli`)
- **`crates/manifest/`** — Shared types for config files and `.stepshot` bundles (`stepshots-manifest`)
- **`extension/`** — Chrome extension for in-browser recording
- **`packages/react/`** — React component (`@stepshots/react`)
- **`packages/tour/`** — Live guided-tour player (`@stepshots/tour`)
- **`examples/`** — Embed integration examples
- **`skills/`** — Claude Code skills for AI-assisted demo creation

## Documentation

- **Website:** [stepshots.com](https://stepshots.com)
- **CLI page:** [stepshots.com/cli](https://stepshots.com/cli)
- **Docs:** [Getting started](https://stepshots.com/docs/getting-started/introduction) and [CLI reference](https://stepshots.com/docs/cli/installation)
- **Blog:** [Guides and comparisons](https://stepshots.com/blog)

## License

MIT
