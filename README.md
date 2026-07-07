# Stepshots

Open-source tools for recording interactive product demos.

This repo contains the **CLI**, **Chrome extension**, and **React SDK** for [Stepshots](https://stepshots.com) — capture step-by-step screenshots, bundle them into `.stepshot` files, and embed them anywhere.

Learn more on the [CLI page](https://stepshots.com/cli), or read the full [documentation](https://stepshots.com/docs/getting-started/introduction).

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

Creates a `stepshots.config.json` with a sample tutorial definition.

### Record tutorials

```sh
# Record all tutorials defined in the config
stepshots record

# Record a specific tutorial
stepshots record --tutorial my-tutorial

# Preview in a visible browser
stepshots preview my-tutorial
```

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
# Upload a recorded bundle
stepshots upload output/my-tutorial.stepshot

# Replace an existing demo
stepshots upload output/my-tutorial.stepshot --demo-id <DEMO_ID>

# Use a custom server
stepshots upload output/my-tutorial.stepshot --server https://your-instance.com
```

Uploads use your `stepshots login` session, or `STEPSHOTS_TOKEN` / `--token` when set. Override the server with `--server` or `STEPSHOTS_SERVER`.

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
- **`examples/`** — Embed integration examples
- **`skills/`** — Claude Code skills for AI-assisted demo creation

## Documentation

- **Website:** [stepshots.com](https://stepshots.com)
- **CLI page:** [stepshots.com/cli](https://stepshots.com/cli)
- **Docs:** [Getting started](https://stepshots.com/docs/getting-started/introduction) and [CLI reference](https://stepshots.com/docs/cli/installation)
- **Blog:** [Guides and comparisons](https://stepshots.com/blog)

## License

MIT
