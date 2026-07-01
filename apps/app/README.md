# LuniVex - Desktop App

The Tauri + Vue desktop app for LuniVex. Built on Modrinth's open-source [Theseus](https://github.com/modrinth/theseus) launcher (GPLv3).

## Prerequisites

- [Node.js](https://nodejs.org/)
- [pnpm](https://pnpm.io/)
- [Rust](https://www.rust-lang.org/tools/install)
- [Tauri CLI](https://tauri.app/start/prerequisites/)

## Setup

Install dependencies from the repo root:

```
pnpm install
```

Start the dev server:

```
pnpm app:dev
```

## Building

For a signed release build with the auto-updater:

```
cargo tauri build --features updater
```

See [RELEASING.md](../../RELEASING.md) for the full release process including signing key setup and publishing a GitHub release.
