# LuniVex Release Process

## Prerequisites

- Rust toolchain installed
- Node.js + pnpm installed
- GitHub CLI (`gh`) authenticated with push access to `LuniVex/lunivex`
- The signing private key (see below)

## Signing Key

The updater signing keypair was generated once and is stored as follows.

**Public key** (committed to `tauri.conf.json` - safe to be public):
```
dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEJFNkIxQjhGNjA1QUMyREYKUldUZndscGdqeHRydmhLd3FJaFN2a05WeXNPNWFXMys5MDhzb1l1NkE3Tlo2RWk1QmtMY3lwRTQK
```

**Private key** - stored securely offline. Never commit this.

Set these as environment variables before building:
```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = "<your private key>"
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ""
```

## Step-by-Step Release

### 1. Bump the version

Edit `apps/app-frontend/package.json` - the `"version"` field there is what `tauri.conf.json` reads via `"version": "../app-frontend/package.json"`.

```json
"version": "1.1.0"
```

Commit the bump:
```powershell
git add apps/app-frontend/package.json
git commit -m "chore: bump version to 1.1.0"
git tag v1.1.0
git push && git push --tags
```

### 2. Set signing environment variables

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = "<private key above>"
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ""
```

### 3. Build the release MSI

```powershell
cd c:\Users\867ki\Projects\lunivex
pnpm prepr:frontend:app
cd apps/app
cargo tauri build --features updater
```

The build output will be at:
```
target/release/bundle/msi/LuniVex_<version>_x64_en-US.msi
```

Tauri automatically produces a `.msi.zip` and a `.msi.zip.sig` (the signature file) alongside it.

### 4. Prepare the update manifest

Copy `release/latest.json.template` to `release/latest.json` and fill in the fields:

| Placeholder | Value |
|-------------|-------|
| `VERSION` | The version string, e.g. `1.1.0` |
| `RELEASE_NOTES` | Short description of what changed |
| `PUB_DATE` | ISO 8601 timestamp, e.g. `2026-06-28T12:00:00Z` |
| `SIGNATURE` | Contents of the `.msi.zip.sig` file (read it as plain text) |

Example for reading the signature:
```powershell
Get-Content "target\release\bundle\msi\LuniVex_1.1.0_x64_en-US.msi.zip.sig"
```

### 5. Create the GitHub release

```powershell
gh release create v1.1.0 `
  "target\release\bundle\msi\LuniVex_1.1.0_x64_en-US.msi" `
  "target\release\bundle\msi\LuniVex_1.1.0_x64_en-US.msi.zip" `
  "target\release\bundle\msi\LuniVex_1.1.0_x64_en-US.msi.zip.sig" `
  "release\latest.json" `
  --title "LuniVex v1.1.0" `
  --notes "Release notes here"
```

The `latest.json` asset attached to the release is what running launchers poll for updates. Tauri fetches it from `releases/latest/download/latest.json` (GitHub automatically redirects `latest/download/` to the newest release's assets).

### 6. Verify

Wait a few minutes, then open a running dev build and confirm the update banner appears on the home page.

## If you need to regenerate the signing key

```powershell
cd c:\Users\867ki\Projects\lunivex
TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" node_modules/.pnpm/node_modules/.bin/tauri signer generate --ci
```

Update `tauri.conf.json` with the new public key. All previously signed releases will stop verifying - users on old builds will need a manual reinstall.
