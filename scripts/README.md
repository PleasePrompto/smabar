# scripts

Every file here is the implementation of a `just` recipe, a CI job or a
release step. Day-to-day work needs only the recipes:

```bash
just dev     # start the app against the Vite dev server (dev.sh; scripts\dev.bat on Windows)
just check   # every quality gate, required before each commit
just build   # release bundles for this machine
```

| Script | Purpose | Called by | Run directly? |
| --- | --- | --- | --- |
| `dev.sh` | The only way to start smabar in dev on Linux/macOS: kills stray instances, checks the desktop is clean, starts exactly one. | `just dev`, `dev-render.sh`, `dev-store.sh`, `memory/` | Through `just dev`. |
| `dev.bat` | The same for Windows. | `scripts\dev.bat` | Yes, on Windows. |
| `dev-render.sh` | Dev start with an explicit Linux WebKit renderer (`auto`, `nvidia`, `software`) for stale-frame checks. | Linux testing docs | Yes, one variant at a time. |
| `dev-store.sh` | Local update server on 127.0.0.1:8787 serving `.dev-store/`, so the updater can be tested without a public release (ADR 0009). | `just dev-store` | Through `just`. |
| `check-file-length.sh` | The 500-line limit for source files. | `just lines` (part of `just check`) | Through `just`. |
| `gen-kit-classes.py` | Generates the UI-kit class documentation from the kit CSS; `--check-local` verifies it is current. | `just check-generated` | Regenerate with `scripts/gen-kit-classes.py`, check through `just`. |
| `gen-google-fonts.py` | Refreshes (`GOOGLE_FONTS_API_KEY`, maintainers) or verifies (`--check`) the checked-in Google Fonts catalog. | `just check-generated` | Refresh: maintainers only. |
| `gen-installer-assets.py` | Generates installer branding (NSIS bitmaps, DMG background); `--check` verifies. | `just check-generated` | Through `just`. |
| `harvest-f48.py` | One-time import of the format48 component CSS into the kit. Still imported by `gen-kit-classes.py` for its token map; do not delete. | `gen-kit-classes.py` | No. |
| `fetch-uv.sh` | Downloads the pinned uv binary for a target triple into `crates/smabar/binaries` with checksum verification, for bundling. | `just build`, CI job Build (bundle), `msix-pack.ps1` | Through `just` or CI. |
| `app-version.sh` | The single source of the app version (Cargo workspace). | CI job Build, `release.py`, `msix-pack.ps1`, `app-archive-macos.sh` | No. |
| `app-archive-macos.sh` | Packs `smabar.app` as the updater archive announced in `latest.json`. | `just build` on macOS, CI job Build | Through `just` or CI. |
| `msix-pack.ps1` | Builds the Microsoft Store (MSIX) variant on Windows; optional local test signing by certificate thumbprint. | CI job Build (Windows), Store testing | Store testers on Windows. |
| `release.py` | Builds the update release tree: verified artifacts plus `latest.json` (ADR 0009). | CI job Build (release), `just dev-store-release` | Through CI or `just`. |
| `release-r2.py` | Publishes verified release bytes to R2; existing version objects stay immutable. | CI workflow Publish updates | No, CI only. |
| `measure-memory.py`, `measure-memory.ps1` | Sample a process tree's memory as JSONL (Linux in KiB, Windows in bytes). | `memory/`, memory testing docs | Yes, for diagnosis. |
| `memory/` | Linux memory harness: cycle windows, inspector accounting, quit loop. See `memory/README.md`. | Manual diagnosis | Yes, for diagnosis. |
| `test_dev.py`, `test_installer_assets.py`, `test_memory.py`, `test_release.py` | unittest suites for the scripts beside them; offline, no running app. | `just check-py` | Through `just`. |
| `test_memory.ps1` | Windows checks of the memory sampler, offline. | Manual on Windows | Yes. |
