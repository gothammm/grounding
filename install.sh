#!/usr/bin/env bash
set -euo pipefail

REPO="gothammm/grounding"
INSTALL_DIR="${INSTALL_DIR:-${HOME}/.local/bin}"
BINARY_NAME="grounding"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log_info() { echo -e "${GREEN}[INFO]${NC} $*"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $*"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*" >&2; }

detect_platform() {
  local os arch
  case "$(uname -s)" in
    Linux*)  os="unknown-linux-gnu";;
    Darwin*) os="apple-darwin";;
    *)
      log_error "Unsupported OS: $(uname -s)"
      exit 1
      ;;
  esac
  case "$(uname -m)" in
    x86_64) arch="x86_64";;
    aarch64|arm64) arch="aarch64";;
    *)
      log_error "Unsupported architecture: $(uname -m)"
      exit 1
      ;;
  esac
  echo "${arch}-${os}"
}

install_binary() {
  local platform version tarball_url checksum_url tmpdir
  platform="$1"
  tmpdir=$(mktemp -d)
  trap 'rm -rf "${tmpdir}"' EXIT

  if [[ -z "${version:-}" ]]; then
    log_info "Fetching latest version..."
    version=$(curl -sSfL "https://api.github.com/repos/${REPO}/releases/latest" \
      | grep '"tag_name":' \
      | sed -E 's/.*"([^"]+)".*/\1/')
    log_info "Latest version: ${version}"
  fi

  tarball_url="https://github.com/${REPO}/releases/download/${version}/grounding-${platform}.tar.gz"
  checksum_url="https://github.com/${REPO}/releases/download/${version}/grounding-${platform}.tar.gz.sha256"

  log_info "Downloading ${version} for ${platform}..."
  curl -sSfL "${tarball_url}" -o "${tmpdir}/grounding.tar.gz"
  curl -sSfL "${checksum_url}" -o "${tmpdir}/grounding.tar.gz.sha256"

  log_info "Verifying checksum..."
  (cd "${tmpdir}" && sha256sum -c grounding.tar.gz.sha256) || {
    log_error "Checksum verification failed! Aborting."
    exit 1
  }

  log_info "Extracting binary..."
  tar -xzf "${tmpdir}/grounding.tar.gz" -C "${tmpdir}"

  mkdir -p "${INSTALL_DIR}"
  mv "${tmpdir}/grounding" "${INSTALL_DIR}/${BINARY_NAME}"
  chmod +x "${INSTALL_DIR}/${BINARY_NAME}"

  log_info "Installed grounding ${version} to ${INSTALL_DIR}/${BINARY_NAME}"

  if ! echo "${PATH}" | tr ':' '\n' | grep -qx "${INSTALL_DIR}"; then
    log_warn "${INSTALL_DIR} is not in your PATH"
    log_info "Add it to your shell profile:"
    log_info "  export PATH=\"\${HOME}/.local/bin:\$PATH\""
  else
    log_info "Run 'grounding --help' to get started"
  fi
}

main() {
  while [[ $# -gt 0 ]]; do
    case $1 in
      --version)
        version="$2"
        shift 2
        ;;
      --dir)
        INSTALL_DIR="$2"
        shift 2
        ;;
      --help|-h)
        echo "Usage: $0 [OPTIONS]"
        echo ""
        echo "Install grounding, a single-binary retrieval engine for LLM context."
        echo ""
        echo "Options:"
        echo "  --version VERSION  Install a specific version (default: latest)"
        echo "  --dir PATH         Install to a specific directory (default: ~/.local/bin)"
        echo "  --help,-h          Show this help message"
        exit 0
        ;;
      *)
        log_error "Unknown option: $1"
        exit 1
        ;;
    esac
  done

  log_info "Detecting platform..."
  local platform
  platform=$(detect_platform)
  log_info "Platform: ${platform}"

  install_binary "${platform}"
}

main "$@"
