# stepshots-cli

[![Crates.io](https://img.shields.io/crates/v/stepshots-cli.svg)](https://crates.io/crates/stepshots-cli)
[![Downloads](https://img.shields.io/crates/d/stepshots-cli.svg)](https://crates.io/crates/stepshots-cli)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/hauju/stepshots/blob/main/LICENSE)

**Record interactive product demos from your terminal.** `stepshots` is an open-source
command-line tool that drives headless Chrome through a flow you describe in JSON, captures a
full-page screenshot per step, and bundles the result into a shareable `.stepshot` demo — the
kind of clickable walkthrough you'd otherwise build by hand in Storylane, Navattic, or Arcade.

Your demo is config-as-code: a `stepshots.config.json` you diff, review in pull requests, and
keep next to the product it documents. When the UI changes, re-run the config and every
screenshot refreshes at once.

- **Website:** [stepshots.com](https://stepshots.com) · **CLI page:** [stepshots.com/cli](https://stepshots.com/cli)
- **Docs:** [Getting started](https://stepshots.com/docs/getting-started/introduction) · [CLI reference](https://stepshots.com/docs/cli/installation)
- **Source:** [github.com/hauju/stepshots](https://github.com/hauju/stepshots) (MIT)

## Install

```sh
cargo install stepshots-cli
```

Installs the `stepshots` binary. Requires Chrome or Chromium on the system — the CLI talks to it
over the DevTools Protocol; it does not download a browser.

Prebuilt binaries for macOS (Apple Silicon) and Linux (x86_64, aarch64):

```sh
curl -sSL https://raw.githubusercontent.com/hauju/stepshots/main/install.sh | sh
```

## Quick start

```sh
stepshots init                  # scaffold stepshots.config.json (JSON Schema included)
stepshots list                  # show the tutorials defined in it
stepshots preview my-tutorial   # watch the flow run in a visible browser
stepshots record my-tutorial    # capture it headlessly -> output/my-tutorial.stepshot
stepshots login                 # browser OAuth, stores a token locally
stepshots upload output/my-tutorial.stepshot --public
```

A minimal config — a start page, a click with a callout, and a typed value. Each step becomes
one screenshot:

```json
{
  "$schema": "https://raw.githubusercontent.com/hauju/stepshots/main/schema/stepshots.config.schema.json",
  "baseUrl": "https://app.example.com",
  "tutorials": {
    "my-tutorial": {
      "url": "/dashboard",
      "title": "Create your first project",
      "steps": [
        {
          "action": "click",
          "selector": "[data-test=new-project]",
          "highlights": [{ "callout": "Start a new project", "position": "bottom" }]
        },
        { "action": "type", "selector": "#name", "text": "Acme" }
      ]
    }
  }
}
```

Actions: `click`, `type`, `key`, `select`, `hover`, `scroll`, `navigate`, `wait`. Steps also carry
blur regions, arrows, hotspots, popups, and zoom regions — annotations baked in at capture time.

Print the full schema with `stepshots schema`, or see the
[configuration reference](https://stepshots.com/docs/cli/configuration).

## Commands

| Command | What it does |
| --- | --- |
| `init` / `schema` | Scaffold a config, or print its JSON Schema |
| `record` / `preview` | Capture a tutorial headlessly, or watch it run in a visible browser |
| `verify` | Replay tutorials against the live app and report drift (CI-friendly exit codes, `--json`) |
| `patch` | Amend an existing `.stepshot` bundle with hand-captured steps (append, insert, replace) |
| `inspect` | Explore a page and find stable selectors |
| `tour` | Scaffold, validate, live-check, build, and push guided tours |
| `login` / `whoami` / `upload` | Authenticate and publish bundles to your workspace |
| `browser` | Open a persistent profile once to record logged-in flows |
| `drift` | Diff DOM extracts against the live app — catches changes no step points at |
| `sandbox` | Generate and publish AI-rebuilt interactive sandboxes from a recording |
| `serve` | Local HTTP server the Chrome recorder extension hands recordings to |
| `doctor` | Check browser, config, server reachability, and login in one pass |
| `mcp` | Serve the recording workflow to AI agents over the Model Context Protocol |
| `completions` / `upgrade` | Shell completions; upgrade in place |

Every command takes `--help`. Global flags: `--config`, `--json` (machine-readable output for
agents and CI), `--connect` (attach to a running Chrome), `--verbose`.

## Keep demos from going stale

Screenshot demos rot the moment the UI moves. `verify` replays each tutorial headless and
reports what broke — selectors that no longer match, start pages that fail to load, annotations
that lost their anchor — without writing bundles:

```sh
stepshots verify --json          # machine-readable report, repair hint per failure
```

Exit code 0 means fresh, 1 means drift. Run it on a schedule with the
[GitHub Action](https://github.com/hauju/stepshots):

```yaml
- uses: hauju/stepshots@main
  with:
    command: verify
    config: demo/stepshots.config.json
```

## Record logged-in flows

Recordings start from a clean browser, so authenticated pages appear logged out. Log in once
into a persistent profile:

```sh
stepshots browser https://app.example.com/login --profile-dir ~/.stepshots/profile
stepshots record my-tutorial --profile-dir ~/.stepshots/profile
```

In CI, hand over the session as JSON instead (Playwright `storageState` format):

```sh
stepshots verify --storage-state auth.json
```

Generate it during the run — it is credentials in a file. Never commit it.

Or attach to a Chrome you are already signed into, instead of launching an automated one:

```sh
# chrome --remote-debugging-port=9222 --user-data-dir=/tmp/stepshots-chrome
stepshots record my-tutorial --connect 9222
```

## Guided tours

The same recording can become a **live guided tour**: an overlay that runs on your real app,
spotlighting the next element and advancing on the user's actual clicks. Tours are text-only
`tours/<key>.tour.json` files you version alongside the app.

```sh
stepshots tour init onboarding --from output/onboarding.stepshot
stepshots tour validate
stepshots tour check --url https://staging.example.com   # live selector check
stepshots tour push                                      # host it, get an embed script
```

Play them with [`@stepshots/tour`](https://www.npmjs.com/package/@stepshots/tour). See the
[guided tours guide](https://stepshots.com/docs/guides/live-tours).

## For AI agents

`stepshots mcp` exposes the workflow as [Model Context Protocol](https://modelcontextprotocol.io)
tools — `get_schema`, `list_tutorials`, `record`, `verify`, `upload` — so an agent can write the
config, record the flow, and publish the demo end to end:

```sh
claude mcp add stepshots -- stepshots mcp
```

## Configuration via environment

| Variable | Purpose |
| --- | --- |
| `STEPSHOTS_TOKEN` | API token for CI (instead of `stepshots login`) |
| `STEPSHOTS_SERVER` | Point at a self-hosted instance |
| `STEPSHOTS_PROFILE_DIR` | Default persistent browser profile |
| `STEPSHOTS_STORAGE_STATE` | Default `storageState` JSON path |

## Where demos live

Recording is free and unlimited — bundles are plain ZIP files (PNG screenshots + a JSON
manifest, typed by the [`stepshots-manifest`](https://crates.io/crates/stepshots-manifest) crate)
that stay on your disk. Uploading to [stepshots.com](https://stepshots.com) adds hosting, an
editor, embeds, and view analytics.

## License

MIT © [Hauke Jung](https://github.com/hauju)
