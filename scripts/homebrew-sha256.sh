#!/usr/bin/env bash
# Compute the GitHub archive sha256 for a flowd tag so Formula/flowd.rb can
# switch from HEAD to a stable url/sha256/version block.
set -euo pipefail

VERSION="${1:-}"
if [[ -z "${VERSION}" ]]; then
  echo "usage: $0 <version>" >&2
  echo "example: $0 1.0.0" >&2
  exit 1
fi

VERSION="${VERSION#v}"
URL="https://github.com/nickvd7/flowd/archive/refs/tags/v${VERSION}.tar.gz"
TMP="$(mktemp)"
cleanup() { rm -f "${TMP}"; }
trap cleanup EXIT

echo "Downloading ${URL}"
curl -fsSL "${URL}" -o "${TMP}"
SUM="$(shasum -a 256 "${TMP}" | awk '{print $1}')"

cat <<EOF
# Paste into Formula/flowd.rb:

  url "${URL}"
  sha256 "${SUM}"
  version "${VERSION}"
EOF
