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

`push` is one-way: your git files stay the source of truth, and pushing overwrites the hosted copy. Hosted tours get anonymous run analytics in the dashboard.

Tag a tutorial with `"target": "tour"` in `stepshots.config.json` and `record` warns about interactive steps without a callout (they'd be dropped from the tour) and scaffolds the tour file automatically. `verify` and `tour check` are siblings: one guards your demos, the other your tours.

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

A framework-agnostic player that runs a guided tour on your real app — spotlighting the next element and advancing on the user's actual clicks and typing. Author tours as `*.tour.json` files (see [Guided tours](#guided-tours)), import them directly with a bundler or emit a registry script with `stepshots tour build`, or let [stepshots.com](https://stepshots.com) host tours for you with a single script tag.

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
