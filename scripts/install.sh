#!/usr/bin/env bash
set -euo pipefail

REPO="${FLOWD_REPO:-nickvd7/flowd}"
PREFIX="${FLOWD_PREFIX:-$HOME/.local}"
BIN_DIR="${PREFIX}/bin"
mkdir -p "${BIN_DIR}"

if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo is required for source install" >&2
  exit 1
fi

TMP="$(mktemp -d)"
cleanup() { rm -rf "${TMP}"; }
trap cleanup EXIT

echo "Installing flowd into ${BIN_DIR}"
git clone --depth 1 "https://github.com/${REPO}.git" "${TMP}/flowd"
(
  cd "${TMP}/flowd"
  cargo install --path crates/flow-cli --root "${PREFIX}" --force
  cargo install --path crates/flow-daemon --root "${PREFIX}" --force
)

# Prefer the flowctl binary name when present.
if [[ -x "${BIN_DIR}/flow-cli" && ! -x "${BIN_DIR}/flowctl" ]]; then
  ln -sf flow-cli "${BIN_DIR}/flowctl"
fi

echo
echo "Installed:"
echo "  ${BIN_DIR}/flowctl (or flow-cli)"
echo "  ${BIN_DIR}/flow-daemon"
echo
echo "Next:"
echo "  flowctl setup --watch ~/Downloads"
echo "  flowctl daemon install-service"
echo "  flowctl daemon start"
echo "  flowctl status"
