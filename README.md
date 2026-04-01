# Stepshots CLI

Record, bundle, and upload interactive product demos from the command line.

Stepshots CLI automates browser interactions via headless Chrome to capture step-by-step screenshots, bundles them into `.stepshot` files, and uploads them to [Stepshots](https://stepshots.com) for sharing and embedding.

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

## Crates

This repository contains two crates:

- **`stepshots-cli`** — The CLI binary
- **`stepshots-manifest`** — Shared types for config files and `.stepshot` bundle manifests

## License

MIT
