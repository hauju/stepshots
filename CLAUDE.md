# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Package Manager

Always use **Bun** for JS/TS packages. Never use npm, yarn, or pnpm.

## Build Commands

### Rust CLI (workspace root)
```sh
cargo build                    # debug build
cargo build --release          # release build
cargo run -- <subcommand>      # run CLI directly (e.g. cargo run -- record -t my-tutorial)
cargo clippy                   # lint
cargo test                     # run all tests
cargo install --path crates/cli  # install locally as `stepshots`
```

### Chrome Extension
```sh
cd extension && bun install && bun run build
```

### React SDK
```sh
cd packages/react && bun install && bun run build
```

## Architecture

This is a Cargo workspace with two crates plus JS/TS packages:

### `crates/manifest/` — `stepshots-manifest`
Shared Rust types used by both the CLI and any consumer of `.stepshot` bundles. Two type hierarchies:
- **Config types** (`StepshotsConfig`, `TutorialConfig`, `StepConfig`) — deserialized from `stepshots.config.json`, reference elements by CSS selectors.
- **Bundle types** (`BundleManifest`, `BundleManifestStep`) — stored inside `.stepshot` ZIP files, use resolved pixel coordinates (`ElementBounds`, `Point2D`) instead of selectors.

All serde uses `camelCase` for JSON interop.

### `crates/cli/` — `stepshots-cli` (binary: `stepshots`)
CLI that records browser interactions into `.stepshot` bundles. Key modules:
- `commands/` — one file per subcommand: `init`, `record`, `preview`, `rerecord`, `upload`, `inspect`, `serve`
- `browser.rs` — Chrome/Chromium automation via `chromiumoxide`
- `actions.rs` — step action execution (click, type, scroll, etc.)
- `bundler.rs` — creates `.stepshot` ZIP bundles (PNG screenshots + JSON manifest)
- `bundle_reader.rs` — reads existing `.stepshot` bundles (for re-record)
- `config.rs` — config file discovery and loading

The CLI requires Chrome/Chromium installed. Set `CHROME_PATH` if not in the default location.

### `extension/` — Chrome Extension
In-browser recorder that captures interactions and sends them to the CLI's `serve` command via HTTP (port 8124). Built with Bun + TypeScript.

### `packages/react/` — `@stepshots/react`
React component for embedding demos. Built with `tsup`, outputs ESM + CJS + types.

### Key data flow
`stepshots.config.json` (selectors) → `record` command → browser automation → screenshots + selector-to-pixel resolution → `.stepshot` ZIP (pixel coordinates) → `upload` to API or embed via React SDK.
