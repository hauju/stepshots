#!/usr/bin/env sh
# Stepshots CLI installer.
#
# Usage:
#   curl -sSL https://raw.githubusercontent.com/hauju/stepshots/main/install.sh | sh
#
# Environment overrides:
#   STEPSHOTS_VERSION    pin a specific release tag (e.g. v1.0.1). Defaults to the latest release.
#   STEPSHOTS_INSTALL_DIR  destination directory. Defaults to $HOME/.local/bin.

set -eu

REPO="hauju/stepshots"
BIN="stepshots"

err() {
  printf "error: %s\n" "$1" >&2
  exit 1
}

need() {
  command -v "$1" >/dev/null 2>&1 || err "missing required command: $1"
}

need curl
need tar
need uname

# --- detect platform ---------------------------------------------------------

uname_s="$(uname -s)"
uname_m="$(uname -m)"

case "$uname_s" in
  Darwin)
    case "$uname_m" in
      arm64|aarch64) target="aarch64-apple-darwin" ;;
      x86_64) err "macOS Intel (x86_64) builds aren't published yet. Install via Rust: cargo install stepshots-cli" ;;
      *) err "unsupported macOS architecture: $uname_m" ;;
    esac
    ;;
  Linux)
    case "$uname_m" in
      x86_64|amd64) target="x86_64-unknown-linux-gnu" ;;
      aarch64|arm64) target="aarch64-unknown-linux-gnu" ;;
      *) err "unsupported Linux architecture: $uname_m" ;;
    esac
    ;;
  *)
    err "unsupported OS: $uname_s. Install via Rust: cargo install stepshots-cli"
    ;;
esac

# --- resolve version ---------------------------------------------------------

version="${STEPSHOTS_VERSION:-}"
if [ -z "$version" ]; then
  version="$(
    curl -sSL -H "Accept: application/vnd.github+json" \
      "https://api.github.com/repos/${REPO}/releases/latest" \
      | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' \
      | head -n1
  )"
  [ -n "$version" ] || err "could not determine latest release tag from github.com/${REPO}"
fi

# Strip leading v if a user passes "1.0.1" instead of "v1.0.1".
version_no_v="${version#v}"

# --- download + verify -------------------------------------------------------

tarball="stepshots-v${version_no_v}-${target}.tar.gz"
url="https://github.com/${REPO}/releases/download/${version}/${tarball}"
sha_url="${url}.sha256"

tmp="$(mktemp -d 2>/dev/null || mktemp -d -t stepshots)"
trap 'rm -rf "$tmp"' EXIT INT TERM

printf "Downloading %s\n" "$tarball"
if ! curl -fSL --progress-bar -o "$tmp/$tarball" "$url"; then
  err "download failed: $url"
fi

if curl -fsSL -o "$tmp/$tarball.sha256" "$sha_url" 2>/dev/null; then
  expected="$(awk '{print $1}' "$tmp/$tarball.sha256")"
  if command -v shasum >/dev/null 2>&1; then
    actual="$(shasum -a 256 "$tmp/$tarball" | awk '{print $1}')"
  elif command -v sha256sum >/dev/null 2>&1; then
    actual="$(sha256sum "$tmp/$tarball" | awk '{print $1}')"
  else
    actual=""
  fi
  if [ -n "$actual" ] && [ "$expected" != "$actual" ]; then
    err "checksum mismatch: expected $expected, got $actual"
  fi
fi

# --- extract + install -------------------------------------------------------

tar -xzf "$tmp/$tarball" -C "$tmp"

# Tarball layout is stepshots-v<version>-<target>/stepshots
src_bin="$tmp/stepshots-v${version_no_v}-${target}/${BIN}"
[ -f "$src_bin" ] || err "binary not found in tarball at $src_bin"

install_dir="${STEPSHOTS_INSTALL_DIR:-$HOME/.local/bin}"
mkdir -p "$install_dir"

dest="$install_dir/$BIN"
mv "$src_bin" "$dest"
chmod +x "$dest"

printf "\nInstalled %s %s to %s\n" "$BIN" "$version" "$dest"

# --- PATH hint ---------------------------------------------------------------

case ":$PATH:" in
  *":$install_dir:"*) ;;
  *)
    printf "\nNote: %s is not on your PATH.\n" "$install_dir"
    printf "Add the following to your shell profile (e.g. ~/.bashrc, ~/.zshrc, ~/.config/fish/config.fish):\n\n"
    printf "  export PATH=\"%s:\$PATH\"\n" "$install_dir"
    ;;
esac

printf "\nNext: run \`stepshots init\` in a project to create stepshots.config.json.\n"
printf "Requires Chrome or Chromium installed.\n"
