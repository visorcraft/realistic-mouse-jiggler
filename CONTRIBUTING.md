# Contributing to Realistic Mouse Jiggler

Thanks for helping improve Realistic Mouse Jiggler. This project is a small
Rust desktop utility, so changes should stay focused, boring, and easy to
review.

If anything here is unclear or out of date, open an issue or a PR.

## Code of conduct

Be kind, be specific, assume good faith. Disagree about the technical details,
not the person. Public reviews stay focused on the diff.

## How to propose a change

Use the standard **fork → branch → pull request** workflow on GitHub.

1. Fork [`visorcraft/realistic-mouse-jiggler`](https://github.com/visorcraft/realistic-mouse-jiggler).
2. Clone your fork and add the upstream remote:

   ```sh
   git clone git@github.com:<you>/realistic-mouse-jiggler.git
   cd realistic-mouse-jiggler
   git remote add upstream https://github.com/visorcraft/realistic-mouse-jiggler.git
   ```

3. Branch from `main` with a descriptive name:

   ```sh
   git fetch upstream
   git switch -c fix-tray-restore upstream/main
   ```

4. Make focused commits. One logical change per commit.
5. Open a pull request against `main` and include:
   - what changed,
   - why it changed,
   - exact test commands run,
   - any platform you could not test.

## Before you push: preflight

Run the core checks locally:

```sh
cargo fmt --check
cargo test --locked
cargo build --release --locked
```

When changing the release workflow, also run:

```sh
actionlint .github/workflows/release.yml .github/workflows/test-azure-signing.yml
```

When changing the Arch package metadata, regenerate `.SRCINFO`:

```sh
(cd packaging/arch && makepkg --printsrcinfo > .SRCINFO)
```

## What we look for in a review

- The change does one thing and does it well.
- Behavior changes include a small regression test where practical.
- Tray, input, and movement changes work while the window is hidden.
- Linux tray support stays on `ksni`; do not add GTK/AppIndicator dependencies.
- Global bindings stay lightweight and do not reintroduce `rdev`, `device_query`,
  or GPL-only hook libraries.
- Documentation is updated when user-visible behavior changes.

## Coding standards

- Rust 2021, MSRV from `Cargo.toml`.
- Use the default `rustfmt` style.
- Prefer the standard library and already-present dependencies.
- Keep UI changes simple and native-looking.
- Avoid speculative abstractions and platform code that is not needed yet.
- `unwrap` / `expect` is acceptable in tests; production paths should return or
  report errors cleanly.

## Commit messages

Use clear Conventional Commit-style subjects:

```text
fix(linux): handle tray actions while hidden
feat(ui): add settings theme selection
docs: add contributing guide
```

Keep the subject imperative, concise, and without a trailing period. Do not add
AI/tool attribution trailers.

## Releases

Maintainer release checklist:

1. Update the version in `Cargo.toml`, `Cargo.lock`, `packaging/arch/PKGBUILD`,
   and `packaging/arch/.SRCINFO`.
2. Run the preflight checks.
3. Commit, then create an annotated tag: `git tag -a vX.Y.Z -m "vX.Y.Z"`.
4. Push `main` and the tag.
5. Create the GitHub release from the tag and wait for the release workflow.

## Security

Do not open a public issue for vulnerabilities. Use GitHub private vulnerability
reporting from the repository **Security** tab. See [`SECURITY.md`](SECURITY.md).

## Licensing

Realistic Mouse Jiggler is licensed under MIT. By contributing, you agree that
your changes are made available under the same license.
