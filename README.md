# Stepshots

Open-source tools for recording interactive product demos.

This repo contains the **CLI** and **Chrome extension** for [Stepshots](https://stepshots.com) — capture step-by-step screenshots, bundle them into `.stepshot` files, and upload them for sharing and embedding.

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

## Project Structure

- **`crates/cli/`** — CLI binary (`stepshots-cli`)
- **`crates/manifest/`** — Shared types for config files and `.stepshot` bundles (`stepshots-manifest`)
- **`extension/`** — Chrome extension for in-browser recording
- **`skills/`** — Claude Code skills for AI-assisted demo creation

## License

MIT
