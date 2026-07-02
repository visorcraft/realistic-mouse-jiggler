# Release Signing

This project signs Windows artifacts with Azure Artifact Signing and release assets with GPG.

## Azure Artifact Signing

Signing is configured in `.github/workflows/release.yml`.

- The `windows-artifacts` job uses `environment: release` and `permissions: id-token: write`.
- Required GitHub Actions secrets: `AZURE_CLIENT_ID`, `AZURE_TENANT_ID`, `AZURE_SUBSCRIPTION_ID`.
- Required GitHub Actions variables:
  - `AZURE_SIGNING_ENDPOINT=https://cus.codesigning.azure.net/`
  - `AZURE_SIGNING_ACCOUNT=VisorCraft`
  - `AZURE_CERT_PROFILE=visorcraft`
- The Entra federated credential for the Azure app must match exactly:
  - issuer: `https://token.actions.githubusercontent.com`
  - audience: `api://AzureADTokenExchange`
  - subject: `repo:visorcraft/realistic-mouse-jiggler:environment:release`

Do not replace OIDC with a client secret or PFX certificate unless explicitly requested.

The workflow signs `target\x86_64-pc-windows-msvc\release\realistic-mouse-jiggler.exe`, verifies it with `Get-AuthenticodeSignature`, builds the MSI with `cargo wix`, signs MSI files in `dist\windows`, and verifies the MSI signature.

Use `.github/workflows/test-azure-signing.yml` as the manual smoke test for Azure login, executable signing, and Authenticode verification.

## GPG Release Signing

Release asset signatures use the VisorCraft Packages key.

- UID: `VisorCraft Packages <packages@visorcraft.com>`
- Fingerprint: `1FEE29F48CBCAEDCA3A8A005ADDE097CAA99B277`
- Public key committed in `packaging/keys/visorcraft-packages.asc`.
- Private-key backup material must stay outside the repository and should not be documented with absolute local paths.
- Required GitHub Actions secrets: `GPG_PRIVATE_KEY`, `GPG_PASSPHRASE`.

Never commit the private key or passphrase. GitHub secrets are write-only and cannot be recovered later.

The `gpg-sign-release-assets` job imports the private key, checks `GPG_KEY_FINGERPRINT`, downloads release assets, removes old `.sig` files, adds `visorcraft-packages.asc`, creates detached signatures for all release assets except `.sig` files and the public key, and uploads signatures with `--clobber`.

When rotating this key, update the offline private-key backup, GitHub secrets, `packaging/keys/visorcraft-packages.asc`, `GPG_KEY_FINGERPRINT` in `.github/workflows/release.yml`, `scripts/install-arch.sh`, and any docs that reference the fingerprint. Do not add private-key backup paths to repository docs.
