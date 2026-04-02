# Stepshots

Open-source tools for recording interactive product demos.

This repo contains the **CLI**, **Chrome extension**, and **React SDK** for [Stepshots](https://stepshots.com) — capture step-by-step screenshots, bundle them into `.stepshot` files, and embed them anywhere.

## Installation

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

### Upload to Stepshots

```sh
# Upload a recorded bundle
stepshots upload output/my-tutorial.stepshot

# Replace an existing demo
stepshots upload output/my-tutorial.stepshot --demo-id <DEMO_ID>

# Use a custom server
stepshots upload output/my-tutorial.stepshot --server https://your-instance.com
```

Set `STEPSHOTS_TOKEN` for authentication and `STEPSHOTS_SERVER` to override the default server URL.

### Re-record an existing bundle

```sh
stepshots rerecord my-tutorial.stepshot
```

## Configuration

Tutorials are defined in `stepshots.config.json`. See `stepshots init` for an example.

## Chrome Extension

The `extension/` directory contains the Stepshots Recorder — a Chrome extension that records interactions directly in the browser.

```sh
cd extension
bun install
bun run build
```

Load the `extension/` folder as an unpacked extension in `chrome://extensions`.

## React SDK

```sh
bun add @stepshots/react
```

```tsx
import { StepshotsDemo } from "@stepshots/react";

<StepshotsDemo demoId="your-demo-id" />
```

See [`packages/react/`](packages/react/) for full props documentation.

## Embed Examples

The `examples/` directory has ready-to-use HTML files showing how to embed Stepshots demos:

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

## License

MIT
