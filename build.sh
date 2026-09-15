#!/usr/bin/env bash
#
# Size-optimised release build of pkgr for Linux (and macOS).
#
# Runs formatting, lint and test gates, then builds the release binary with the
# size profile from Cargo.toml, copies it to bin/, and reports its size.
#
# The optimisation settings live in Cargo.toml under [profile.release], so a
# plain `cargo build --release` produces the same binary. This script refuses
# to build if any of them have gone missing, so a stray edit cannot silently
# produce a bloated release.
#
# Usage:
#   ./build.sh
#   ./build.sh --skip-checks
#   ./build.sh --install-dir ~/.local/bin

set -euo pipefail

usage() {
    cat <<'EOF'
Usage: ./build.sh [--skip-checks] [--install-dir DIR]

  --skip-checks       build straight away, without the fmt, clippy and test gates
  --install-dir DIR   copy the finished binary to DIR, e.g. ~/.local/bin
  -h, --help          show this help
EOF
}

skip_checks=false
install_dir=""

while [ $# -gt 0 ]; do
    case "$1" in
        --skip-checks) skip_checks=true ;;
        --install-dir)
            [ $# -ge 2 ] || { echo "error: --install-dir needs a directory" >&2; exit 2; }
            install_dir="$2"
            shift
            ;;
        -h | --help) usage; exit 0 ;;
        *) echo "error: unknown argument: $1" >&2; usage >&2; exit 2 ;;
    esac
    shift
done

cd "$(dirname "$0")"

if [ -t 1 ]; then
    cyan=$'\033[36m' green=$'\033[32m' red=$'\033[31m' reset=$'\033[0m'
else
    cyan="" green="" red="" reset=""
fi

step() { printf '%s%s%s\n' "$cyan" "$1" "$reset"; }
fail() { printf '%serror: %s%s\n' "$red" "$1" "$reset" >&2; exit 1; }

command -v cargo >/dev/null 2>&1 \
    || fail "cargo not found. Install Rust from https://rustup.rs"

# Rust links through the system C toolchain on Linux, so a missing cc only
# surfaces at the very end of a long build unless it is checked up front.
if [ "$(uname -s)" = "Linux" ]; then
    command -v cc >/dev/null 2>&1 \
        || fail "no C linker (cc) found. Install one, e.g. 'sudo apt install build-essential'"
fi

# The release profile is what makes the binary small. Every one of these is
# required. Carriage returns are stripped so a Cargo.toml checked out with
# Windows line endings still matches.
profile="$(tr -d '\r' < Cargo.toml | awk '/^\[profile\.release\]/ { inside = 1; next } /^\[/ { inside = 0 } inside')"
version="$(tr -d '\r' < Cargo.toml | sed -n 's/^version[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)"

required=(
    'opt-level="z"'
    'lto="fat"'
    'codegen-units=1'
    'panic="abort"'
    'strip=true'
)

missing=()
for setting in "${required[@]}"; do
    key="${setting%%=*}"
    value="${setting#*=}"
    # The expected values contain only letters, digits and quotes, none of
    # which are regex metacharacters, so they are safe to embed unescaped.
    pattern="^[[:space:]]*${key}[[:space:]]*=[[:space:]]*${value}[[:space:]]*(#.*)?$"
    grep -Eq "$pattern" <<<"$profile" || missing+=("$key = $value")
done

if [ ${#missing[@]} -gt 0 ]; then
    printf '%serror: Cargo.toml [profile.release] is missing required size settings:%s\n' "$red" "$reset" >&2
    printf '  %s\n' "${missing[@]}" >&2
    exit 1
fi

step "pkgr ${version} - release profile:"
for setting in "${required[@]}"; do
    printf '  %-14s %s\n' "${setting%%=*}" "${setting#*=}"
done
echo

if [ "$skip_checks" = false ]; then
    step "Checking formatting ..."
    cargo fmt --check

    step "Linting ..."
    cargo clippy --release --all-targets --quiet -- -D warnings

    step "Testing ..."
    cargo test --quiet
    echo
fi

step "Building release ..."
# --locked builds exactly the dependency versions in Cargo.lock, so a release
# never quietly picks up a newer crate.
cargo build --release --locked

case "${OSTYPE:-}" in
    msys* | cygwin* | win32*) name="pkgr.exe" ;;
    *) name="pkgr" ;;
esac

# Stable cargo cannot redirect only the final artifact (--artifact-dir is
# nightly), so the release binary is copied out of target/ into bin/.
mkdir -p bin
cp -f "target/release/${name}" "bin/${name}" \
    || fail "could not copy to bin/${name} - is pkgr currently running?"

# wc -c rather than stat, whose flags differ between GNU and BSD/macOS.
size="$(wc -c < "bin/${name}" | tr -d '[:space:]')"

echo
printf '%sBuilt %s/bin/%s  %s bytes (%s KB)%s\n' \
    "$green" "$(pwd)" "$name" "$size" "$((size / 1000))" "$reset"

if [ -n "$install_dir" ]; then
    mkdir -p "$install_dir"
    cp -f "bin/${name}" "${install_dir}/${name}" \
        || fail "could not copy to ${install_dir}/${name} - is pkgr currently running?"
    chmod +x "${install_dir}/${name}"
    printf '%sInstalled to %s/%s%s\n' "$green" "$install_dir" "$name" "$reset"
fi
