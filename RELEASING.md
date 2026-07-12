# Releasing

This repo ships two independently-versioned things.

| Component | Version source of truth | Tag | Published to |
|-----------|-------------------------|-----|--------------|
| CLI + Chrome extension (the "recorder", released together) | `Cargo.toml` (`workspace.package.version`) **and** `extension/manifest.json` — keep them equal | `vX.Y.Z` | GitHub Release (`.github/workflows/release.yml`) + crates.io (hand-published) |
| `@stepshots/react` SDK | `packages/react/package.json` | `react-vX.Y.Z` | npm (hand-published) |

**Rule:** the committed version *is* the source of truth; the tag only has to agree with it.
The CLI enforces this — `cargo build` embeds the `Cargo.toml` version into the binary, so a
tag can never override it. The `check-versions` job in `release.yml` fails the release unless
the tag equals both the Cargo workspace version and `extension/manifest.json`.

`vX.Y.Z` and `react-vX.Y.Z` are separate tag namespaces; the `v*` release workflow does not
fire on `react-v*` tags.

## Cut a CLI + extension release (`vX.Y.Z`)

1. Bump **both** to the new version:
   - `Cargo.toml` → `[workspace.package] version = "X.Y.Z"` (covers `stepshots-cli` and `stepshots-manifest`)
   - `extension/manifest.json` → `"version": "X.Y.Z"` (and `extension/package.json` to match)
   - If the CLI started using new `stepshots-manifest` APIs or features since the last release,
     also raise the `version` floor on the `manifest` dependency in `crates/cli/Cargo.toml` —
     it does **not** move with the workspace bump (see step 7).
2. Refresh the lockfile: `cargo update -w`
3. Commit: `chore(release): bump to X.Y.Z`
4. Tag and push:
   ```sh
   git tag -a vX.Y.Z -m "Release vX.Y.Z"
   git push && git push origin vX.Y.Z
   ```
5. The `Release` workflow builds the CLI (mac-arm, linux-x64, linux-arm) and the extension zip,
   then opens a **draft** GitHub Release. Review the generated notes and publish it.
6. Upload the extension zip to the Chrome Web Store manually if needed.
7. Publish to crates.io — the workflow does **not** do this. Publish in dependency order:
   ```sh
   cargo publish -p stepshots-manifest
   cargo publish -p stepshots-cli
   ```
   `stepshots-manifest` must land first: `stepshots-cli`'s registry dependency on it has to
   resolve. That dependency carries an explicit `version` floor in `crates/cli/Cargo.toml`
   (required for crates.io) — if the CLI uses manifest APIs or features newer than the floor,
   raise it to the version that introduced them, or downstream builds can resolve a manifest
   release that lacks them.

If the tag and committed versions disagree, `check-versions` fails before anything builds — fix
the versions (or the tag) and retry.

## Release the React SDK (`react-vX.Y.Z`)

Currently hand-published (no workflow yet):

```sh
cd packages/react
# bump "version" in package.json first
bun install && bun run build
bun publish            # or: npm publish --access public
git tag -a react-vX.Y.Z -m "React SDK vX.Y.Z"
git push origin react-vX.Y.Z
```

## Versioning policy

Bump only what changed. A component's version moves when its own code changes — don't bump the
SDK for a CLI-only change, and vice versa. The CLI and extension share the `vX.Y.Z` number
because they're coupled and released together.
