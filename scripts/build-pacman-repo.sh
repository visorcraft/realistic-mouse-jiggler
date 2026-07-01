#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
repo_name="${RMJ_PACMAN_REPO_NAME:-realistic-mouse-jiggler}"
arch="${RMJ_PACMAN_ARCH:-x86_64}"
source_dir="${1:-${repo_root}/dist/arch}"
repo_dir="${2:-${repo_root}/dist/pacman/${arch}}"

if ! command -v repo-add >/dev/null 2>&1; then
  echo "repo-add is required. Install pacman-contrib or run this on Arch/CachyOS." >&2
  exit 1
fi

mkdir -p "${repo_dir}"
shopt -s nullglob
packages=("${source_dir}"/*.pkg.tar.zst "${source_dir}"/*.pkg.tar.xz "${source_dir}"/*.pkg.tar.gz)

if (( ${#packages[@]} == 0 )); then
  echo "no package artifacts found in ${source_dir}" >&2
  exit 1
fi

rm -f \
  "${repo_dir}/${repo_name}-"*.pkg.tar.* \
  "${repo_dir}/${repo_name}.db"* \
  "${repo_dir}/${repo_name}.files"*

cp -f "${packages[@]}" "${repo_dir}/"

(
  cd "${repo_dir}"
  repo-add "${repo_name}.db.tar.gz" ./*.pkg.tar.*
  rm -f "${repo_name}.db.tar.gz.old" "${repo_name}.files.tar.gz.old"
)

printf 'Built pacman repo in %s\n' "${repo_dir}"
printf 'pacman Server line: Server = https://<host>/%s/$arch\n' "${repo_name}"
