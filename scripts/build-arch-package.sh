#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
app_name="realistic-mouse-jiggler"
pkg_dir="${repo_root}/packaging/arch"
dist_dir="${repo_root}/dist/arch"
pkgver="$(awk -F= '/^pkgver=/{print $2; exit}' "${pkg_dir}/PKGBUILD")"

if ! command -v makepkg >/dev/null 2>&1; then
  echo "makepkg is required. Run this on Arch/CachyOS or inside an Arch container." >&2
  exit 1
fi

tmp_dir=""
cleanup() {
  if [[ -n "${tmp_dir}" ]]; then
    rm -rf "${tmp_dir}"
  fi
}
trap cleanup EXIT

if [[ -n "$(git -C "${repo_root}" status --porcelain)" ]]; then
  tmp_dir="$(mktemp -d)"
  source_root="${tmp_dir}/${app_name}"
  archive="${tmp_dir}/${app_name}-${pkgver}.tar.gz"

  rsync -a \
    --exclude '.git/' \
    --exclude 'target/' \
    --exclude 'dist/' \
    --exclude 'packaging/arch/pkg/' \
    --exclude 'packaging/arch/src/' \
    --exclude 'packaging/arch/*.pkg.tar.*' \
    --exclude 'packaging/arch/*.src.tar.*' \
    "${repo_root}/" "${source_root}/"
  tar -C "${tmp_dir}" -czf "${archive}" "${app_name}"

  export RMJ_SOURCE="file://${archive}"
  export RMJ_SOURCE_DIR="${app_name}"
else
  commit="$(git -C "${repo_root}" rev-parse HEAD)"
  export RMJ_SOURCE="${app_name}::git+file://${repo_root}#commit=${commit}"
  export RMJ_SOURCE_DIR="${app_name}"
fi

makepkg_args=(-f --clean --cleanbuild)
if [[ "${1:-}" == "--syncdeps" ]]; then
  makepkg_args+=(--syncdeps)
  shift
fi

(
  cd "${pkg_dir}"
  rm -f "${app_name}-"*.pkg.tar.* "${app_name}-"*.tar.gz
  makepkg "${makepkg_args[@]}" "$@"
)

mkdir -p "${dist_dir}"
rm -f "${dist_dir}/${app_name}-"*.pkg.tar.*
cp -f "${pkg_dir}/${app_name}-"*.pkg.tar.* "${dist_dir}/"
printf 'Built package artifacts in %s\n' "${dist_dir}"
