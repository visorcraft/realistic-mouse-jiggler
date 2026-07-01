# CachyOS and Pacman Packaging

This project ships Arch/CachyOS packaging in `packaging/arch`.

## Recommended Distribution Path

For a small upstream desktop app, the practical path is:

1. Publish GitHub releases with binaries and a pacman package asset.
2. Host a small pacman repository yourself once the repo is public.
3. Submit a CachyOS package request or PR later if there is user demand.

CachyOS' package repositories are curated. A self-hosted pacman repo gives users the same `pacman -S` install flow without requiring CachyOS to accept the package first.

## Build a Local Package

On CachyOS or Arch:

```bash
scripts/build-arch-package.sh --syncdeps
```

The package artifact is written to:

```text
dist/arch/
```

Install it locally:

```bash
sudo pacman -U dist/arch/realistic-mouse-jiggler-*.pkg.tar.*
```

## Build a Pacman Repo Directory

After building a package:

```bash
scripts/build-pacman-repo.sh
```

That creates:

```text
dist/pacman/x86_64/
```

Upload that directory to static hosting. Users can then add:

```ini
[realistic-mouse-jiggler]
SigLevel = Optional TrustAll
Server = https://<your-host>/realistic-mouse-jiggler/$arch
```

Then they can install with:

```bash
sudo pacman -Sy realistic-mouse-jiggler
```

Use signed packages and a stricter `SigLevel` before treating the repository as production infrastructure.

## GitHub Pages Hosting

GitHub Pages is a good fit for the pacman repo directory, but the repository or Pages site must be publicly reachable for normal users. This repository is currently private, so a Pages pacman repo would not be a public install source until visibility/Pages settings change.

## CachyOS Submission

If you want to try the official CachyOS route later:

1. Fork `https://github.com/CachyOS/Cachyos-pkgbuilds`.
2. Add a package directory using the `packaging/arch/PKGBUILD` here as the starting point.
3. Generate `.SRCINFO` with `makepkg --printsrcinfo > .SRCINFO`.
4. Open a pull request and be prepared for maintainers to request dependency, signing, or source-verification changes.
