#!/usr/bin/env sh
# install.sh — graphtor-docs installer for macOS and Linux
#
# Usage:
#   curl -sSf https://raw.githubusercontent.com/softwaresalt/graphtor-docs/main/install.sh | sh
#
# Environment variables (optional overrides):
#   GRAPHTOR_INSTALL_DIR  — where to place the binary (default: ~/.local/bin)
#   GRAPHTOR_VERSION      — pin a specific release tag  (default: latest stable)
#
# The installer:
#   1. Detects OS and CPU architecture
#   2. Maps them to the corresponding Rust target triple
#   3. Fetches the latest stable GitHub release tag (or uses GRAPHTOR_VERSION)
#   4. Downloads the .tar.gz archive and SHA256SUMS
#   5. Verifies the checksum
#   6. Extracts the binary to GRAPHTOR_INSTALL_DIR
#   7. Prints PATH guidance if the install dir is not already on PATH

set -eu

REPO="softwaresalt/graphtor-docs"
BINARY="graphtor-docs"

# ── Helpers ──────────────────────────────────────────────────────────────────

info()  { printf '\033[0;34m[graphtor]\033[0m %s\n' "$*"; }
warn()  { printf '\033[0;33m[graphtor]\033[0m %s\n' "$*" >&2; }
error() { printf '\033[0;31m[graphtor]\033[0m %s\n' "$*" >&2; exit 1; }

need() {
    command -v "$1" >/dev/null 2>&1 || error "Required tool not found: $1. Please install it and try again."
}

# ── Dependency check ──────────────────────────────────────────────────────────
need curl
need tar

# Detect the checksum command available on this platform.
# macOS ships shasum; most Linux distributions ship sha256sum.
if command -v sha256sum >/dev/null 2>&1; then
    SHASUM_CMD="sha256sum"
elif command -v shasum >/dev/null 2>&1; then
    SHASUM_CMD="shasum -a 256"
else
    error "sha256sum or shasum is required but not found. Please install one and try again."
fi

# ── OS / arch detection ───────────────────────────────────────────────────────
OS="$(uname -s)"
ARCH="$(uname -m)"

case "${OS}" in
    Linux)
        case "${ARCH}" in
            x86_64)  TARGET="x86_64-unknown-linux-gnu" ;;
            aarch64)
                error "Pre-built Linux/aarch64 binaries are not yet published. Build from source instead:
  cargo install --git https://github.com/softwaresalt/graphtor-docs --bin graphtor-docs --locked"
                ;;
            *) error "Unsupported Linux architecture: ${ARCH}" ;;
        esac
        ;;
    Darwin)
        case "${ARCH}" in
            arm64)   TARGET="aarch64-apple-darwin" ;;
            x86_64)  TARGET="x86_64-apple-darwin" ;;
            *) error "Unsupported macOS architecture: ${ARCH}" ;;
        esac
        ;;
    *) error "Unsupported operating system: ${OS}" ;;
esac

info "Detected platform: ${OS}/${ARCH} → target ${TARGET}"

# ── Version resolution ────────────────────────────────────────────────────────
if [ -z "${GRAPHTOR_VERSION:-}" ]; then
    info "Resolving latest stable release…"
    GRAPHTOR_VERSION="$(
        curl -sSf "https://api.github.com/repos/${REPO}/releases/latest" \
          | grep '"tag_name"' \
          | head -1 \
          | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/'
    )"
fi

[ -n "${GRAPHTOR_VERSION}" ] || error "Could not determine the latest release tag. Check your network connection."

info "Installing ${BINARY} ${GRAPHTOR_VERSION}…"

# ── Install directory ─────────────────────────────────────────────────────────
INSTALL_DIR="${GRAPHTOR_INSTALL_DIR:-${HOME}/.local/bin}"
mkdir -p "${INSTALL_DIR}"

# ── Download ──────────────────────────────────────────────────────────────────
BASE_URL="https://github.com/${REPO}/releases/download/${GRAPHTOR_VERSION}"
ARCHIVE="${BINARY}-${GRAPHTOR_VERSION}-${TARGET}.tar.gz"
SUMS_FILE="SHA256SUMS"

TMP="$(mktemp -d)"
trap 'rm -rf "${TMP}"' EXIT

info "Downloading ${ARCHIVE}…"
curl -sSfL "${BASE_URL}/${ARCHIVE}"   -o "${TMP}/${ARCHIVE}"
curl -sSfL "${BASE_URL}/${SUMS_FILE}" -o "${TMP}/${SUMS_FILE}"

# ── Checksum verification ─────────────────────────────────────────────────────
info "Verifying checksum…"
cd "${TMP}"

EXPECTED_LINE="$(grep "${ARCHIVE}" "${SUMS_FILE}" || true)"
if [ -z "${EXPECTED_LINE}" ]; then
    error "No checksum entry found for ${ARCHIVE} in SHA256SUMS. Cannot verify download integrity."
fi

printf '%s\n' "${EXPECTED_LINE}" | ${SHASUM_CMD} --check --status \
    || error "Checksum verification failed for ${ARCHIVE}. The download may be corrupt or tampered with."

info "Checksum OK."

# ── Extract and install ───────────────────────────────────────────────────────
info "Extracting to ${INSTALL_DIR}…"
tar -xzf "${ARCHIVE}" -C "${TMP}" "${BINARY}"
install -m 0755 "${TMP}/${BINARY}" "${INSTALL_DIR}/${BINARY}"

# ── PATH guidance ─────────────────────────────────────────────────────────────
case ":${PATH}:" in
    *":${INSTALL_DIR}:"*)
        info "Installation complete: ${INSTALL_DIR}/${BINARY}"
        ;;
    *)
        info "Installation complete: ${INSTALL_DIR}/${BINARY}"
        warn ""
        warn "${INSTALL_DIR} is not on your PATH."
        warn "Add it by appending one of the following lines to your shell config:"
        warn ""
        warn "  # ~/.bashrc or ~/.bash_profile"
        warn "  export PATH=\"${INSTALL_DIR}:\$PATH\""
        warn ""
        warn "  # ~/.zshrc"
        warn "  export PATH=\"${INSTALL_DIR}:\$PATH\""
        warn ""
        warn "Then restart your terminal or run: source ~/.bashrc"
        ;;
esac

info "Run '${BINARY} --help' to get started."
