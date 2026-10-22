# shellcheck shell=bash
# Shared setup for the packaging scripts. Source it: `. "$(dirname "$0")/../env.sh"`.
#
# Exports:
#   ROOT                    workspace root
#   VERSION                 [workspace.package] version from Cargo.toml (override: PHOTOCRAFT_VERSION)
#   DIST                    output directory for release artifacts (default: $ROOT/dist/release)
#   PHOTOCRAFT_BUILD_SHA    git commit baked into the binaries (see crates/engine/src/build_info.rs)
#   PHOTOCRAFT_BUILD_DATE   UTC build date, YYYY-MM-DD
#   CARGO_TARGET_DIR        cargo's target dir (default: $ROOT/target)

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export ROOT

# The version lives in exactly one place: `[workspace.package] version` in the root Cargo.toml.
# (`cargo xtask version` prints the same thing; awk avoids compiling xtask here.)
workspace_version() {
  awk '
    /^\[/ { in_pkg = ($0 == "[workspace.package]") ; next }
    in_pkg && $1 == "version" { gsub(/[" ]/, "", $3); print $3; exit }
  ' "$ROOT/Cargo.toml"
}

VERSION="${PHOTOCRAFT_VERSION:-$(workspace_version)}"
if [ -z "$VERSION" ]; then
  echo "error: could not read [workspace.package] version from $ROOT/Cargo.toml" >&2
  exit 1
fi
export VERSION

DIST="${DIST:-$ROOT/dist/release}"
mkdir -p "$DIST"
export DIST

if [ -z "${PHOTOCRAFT_BUILD_SHA:-}" ]; then
  PHOTOCRAFT_BUILD_SHA="$(git -C "$ROOT" rev-parse HEAD 2>/dev/null || true)"
fi
export PHOTOCRAFT_BUILD_SHA
export PHOTOCRAFT_BUILD_DATE="${PHOTOCRAFT_BUILD_DATE:-$(date -u +%Y-%m-%d)}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"

# Emit a GitHub Actions warning (plain stderr outside Actions).
warn() {
  if [ -n "${GITHUB_ACTIONS:-}" ]; then echo "::warning::$*"; else echo "warning: $*" >&2; fi
}

# Copy licence and readme files that exist into a package directory.
copy_docs() {
  local dest="$1" f
  # Full frozen-lock notices; keep application-relative links within Licenses.
  if [ ! -f "$ROOT/NOTICE" ]; then
    echo "error: missing legal notice: $ROOT/NOTICE" >&2
    return 1
  fi
  mkdir -p "$dest/Licenses/." || return 1
  install -m 0644 "$ROOT/NOTICE" "$dest/Licenses/NOTICE" || return 1
  if [ ! -f "$ROOT/ATTRIBUTION.md" ]; then
    echo "error: missing legal notice: $ROOT/ATTRIBUTION.md" >&2
    return 1
  fi
  mkdir -p "$dest/Licenses/." || return 1
  install -m 0644 "$ROOT/ATTRIBUTION.md" "$dest/Licenses/ATTRIBUTION.md" || return 1
  if [ ! -f "$ROOT/assets/icons/LICENSE-noun-magnetic-lasso.txt" ]; then
    echo "error: missing legal notice: $ROOT/assets/icons/LICENSE-noun-magnetic-lasso.txt" >&2
    return 1
  fi
  mkdir -p "$dest/Licenses/assets/icons" || return 1
  install -m 0644 "$ROOT/assets/icons/LICENSE-noun-magnetic-lasso.txt" "$dest/Licenses/assets/icons/LICENSE-noun-magnetic-lasso.txt" || return 1
  if [ ! -f "$ROOT/assets/icons/LICENSE-symmetry-axes.txt" ]; then
    echo "error: missing legal notice: $ROOT/assets/icons/LICENSE-symmetry-axes.txt" >&2
    return 1
  fi
  mkdir -p "$dest/Licenses/assets/icons" || return 1
  install -m 0644 "$ROOT/assets/icons/LICENSE-symmetry-axes.txt" "$dest/Licenses/assets/icons/LICENSE-symmetry-axes.txt" || return 1
  if [ ! -f "$ROOT/crates/ui-egui/src/LICENSE-solarized.txt" ]; then
    echo "error: missing legal notice: $ROOT/crates/ui-egui/src/LICENSE-solarized.txt" >&2
    return 1
  fi
  mkdir -p "$dest/Licenses/crates/ui-egui/src" || return 1
  install -m 0644 "$ROOT/crates/ui-egui/src/LICENSE-solarized.txt" "$dest/Licenses/crates/ui-egui/src/LICENSE-solarized.txt" || return 1
  if [ ! -f "$ROOT/packaging/licenses/epaint_default_fonts-0.36.2/Hack-Regular.txt" ]; then
    echo "error: missing legal notice: $ROOT/packaging/licenses/epaint_default_fonts-0.36.2/Hack-Regular.txt" >&2
    return 1
  fi
  mkdir -p "$dest/Licenses/epaint_default_fonts-0.36.2" || return 1
  install -m 0644 "$ROOT/packaging/licenses/epaint_default_fonts-0.36.2/Hack-Regular.txt" "$dest/Licenses/epaint_default_fonts-0.36.2/Hack-Regular.txt" || return 1
  if [ ! -f "$ROOT/packaging/licenses/epaint_default_fonts-0.36.2/OFL.txt" ]; then
    echo "error: missing legal notice: $ROOT/packaging/licenses/epaint_default_fonts-0.36.2/OFL.txt" >&2
    return 1
  fi
  mkdir -p "$dest/Licenses/epaint_default_fonts-0.36.2" || return 1
  install -m 0644 "$ROOT/packaging/licenses/epaint_default_fonts-0.36.2/OFL.txt" "$dest/Licenses/epaint_default_fonts-0.36.2/OFL.txt" || return 1
  if [ ! -f "$ROOT/packaging/licenses/epaint_default_fonts-0.36.2/UFL.txt" ]; then
    echo "error: missing legal notice: $ROOT/packaging/licenses/epaint_default_fonts-0.36.2/UFL.txt" >&2
    return 1
  fi
  mkdir -p "$dest/Licenses/epaint_default_fonts-0.36.2" || return 1
  install -m 0644 "$ROOT/packaging/licenses/epaint_default_fonts-0.36.2/UFL.txt" "$dest/Licenses/epaint_default_fonts-0.36.2/UFL.txt" || return 1
  if [ ! -f "$ROOT/packaging/licenses/epaint_default_fonts-0.36.2/emoji-icon-font-mit-license.txt" ]; then
    echo "error: missing legal notice: $ROOT/packaging/licenses/epaint_default_fonts-0.36.2/emoji-icon-font-mit-license.txt" >&2
    return 1
  fi
  mkdir -p "$dest/Licenses/epaint_default_fonts-0.36.2" || return 1
  install -m 0644 "$ROOT/packaging/licenses/epaint_default_fonts-0.36.2/emoji-icon-font-mit-license.txt" "$dest/Licenses/epaint_default_fonts-0.36.2/emoji-icon-font-mit-license.txt" || return 1
  for f in README.md LICENSE LICENSE-MIT LICENSE-APACHE COPYRIGHT; do
    if [ -f "$ROOT/$f" ]; then cp "$ROOT/$f" "$dest/"; fi
  done
  copy_font_licences "$dest"
}

# Builds made with craft-fonts (CRAFT_FONTS_DIR, all official releases) embed its fonts, which
# are OFL-1.1: ship each font's licence as OFL-<family-dir>.txt. No-op without CRAFT_FONTS_DIR.
copy_font_licences() {
  local dest="$1" lic fam
  [ -n "${CRAFT_FONTS_DIR:-}" ] || return 0
  for lic in "$CRAFT_FONTS_DIR"/fonts/*/OFL.txt; do
    [ -f "$lic" ] || continue
    fam="$(basename "$(dirname "$lic")")"
    cp "$lic" "$dest/OFL-$fam.txt"
  done
}

# Portable SHA-256 of a file (prints just the hash).
sha256() {
  if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi
}
