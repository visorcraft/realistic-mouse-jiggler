#!/usr/bin/env bash
set -euo pipefail

repo_url="https://github.com/visorcraft/realistic-mouse-jiggler/releases/latest/download"
key_fingerprint="1FEE29F48CBCAEDCA3A8A005ADDE097CAA99B277"
key_url="${repo_url}/visorcraft-packages.asc"
package_url="${repo_url}/realistic-mouse-jiggler-x86_64.pkg.tar.zst"

if ! command -v pacman >/dev/null 2>&1; then
  echo "This installer requires pacman." >&2
  exit 1
fi

if ! command -v curl >/dev/null 2>&1; then
  echo "This installer requires curl." >&2
  exit 1
fi

if ! command -v gpg >/dev/null 2>&1; then
  echo "This installer requires gpg." >&2
  exit 1
fi

workdir="$(mktemp -d)"
trap 'rm -rf "${workdir}"' EXIT

key_file="${workdir}/visorcraft-packages.asc"
curl -fsSLo "${key_file}" "${key_url}"

actual_fingerprint="$(
  gpg --batch --with-colons --show-keys "${key_file}" |
    awk -F: '/^fpr:/ {print $10; exit}'
)"

if [[ "${actual_fingerprint}" != "${key_fingerprint}" ]]; then
  echo "Unexpected VisorCraft package key fingerprint: ${actual_fingerprint}" >&2
  echo "Expected: ${key_fingerprint}" >&2
  exit 1
fi

sudo pacman-key --add "${key_file}"
sudo pacman-key --lsign-key "${key_fingerprint}"
sudo pacman -U --needed --noconfirm "${package_url}"
