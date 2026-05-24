#!/usr/bin/env bash
set -euo pipefail

REPO="gothammm/grounding"
INSTALL_DIR="${INSTALL_DIR:-${HOME}/.local/bin}"
BINARY_NAME="grounding"

RED='\033[0;31m'
NC='\033[0m'

log_error() { echo -e "${RED}error${NC} $*" >&2; }

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
  trap 'rm -rf "${tmpdir:-}"' EXIT

  if [[ -z "${version:-}" ]]; then
    echo "  Fetching latest version..."
    version=$(curl -sL "https://api.github.com/repos/${REPO}/releases/latest" \
      | grep '"tag_name":' \
      | sed -E 's/.*"([^"]+)".*/\1/' || true)

    if [[ -z "${version:-}" ]]; then
      echo "  Could not fetch latest release, trying tags..."
      version=$(curl -sL "https://api.github.com/repos/${REPO}/tags" \
        | grep '"name":' \
        | head -1 \
        | sed -E 's/.*"([^"]+)".*/\1/' || true)
    fi

    if [[ -z "${version:-}" ]]; then
      echo "  Could not determine latest version."
      echo "  Use --version VERSION to specify one manually."
      exit 1
    fi

    echo "  Latest: ${version}"
  fi

  local tarball_name="grounding-${platform}.tar.gz"
  tarball_url="https://github.com/${REPO}/releases/download/${version}/${tarball_name}"
  checksum_url="https://github.com/${REPO}/releases/download/${version}/${tarball_name}.sha256"

  echo "  Downloading ${version} for ${platform}..."
  curl -fL --progress-bar "${tarball_url}" -o "${tmpdir}/${tarball_name}"
  curl -sL "${checksum_url}" -o "${tmpdir}/${tarball_name}.sha256"

  echo "  Verifying checksum..."
  if command -v sha256sum &>/dev/null; then
    (cd "${tmpdir}" && sha256sum -c "${tarball_name}.sha256")
  elif command -v shasum &>/dev/null; then
    (cd "${tmpdir}" && shasum -a 256 -c "${tarball_name}.sha256")
  else
    log_error "No checksum tool found (sha256sum or shasum required)"
    exit 1
  fi || {
    log_error "Checksum verification failed! Aborting."
    exit 1
  }

  echo "  Extracting binary..."
  tar -xzf "${tmpdir}/${tarball_name}" -C "${tmpdir}"

  mkdir -p "${INSTALL_DIR}"
  mv "${tmpdir}/grounding" "${INSTALL_DIR}/${BINARY_NAME}"
  chmod +x "${INSTALL_DIR}/${BINARY_NAME}"

  echo ""
  echo "  Grounding ${version} installed to ${INSTALL_DIR}/${BINARY_NAME}"

  if ! echo "${PATH}" | tr ':' '\n' | grep -qx "${INSTALL_DIR}"; then
    echo "  ${INSTALL_DIR} is not in your PATH"
    echo "  Add it: export PATH=\"\${HOME}/.local/bin:\$PATH\""
  else
    echo "  Run 'grounding --help' to get started"
  fi
}

main() {
  local version=""

  echo "Grounding installer"
  echo ""

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

  echo "  Detecting platform..."
  local platform
  platform=$(detect_platform)
  echo "  Platform: ${platform}"

  install_binary "${platform}"
}

main "$@"
