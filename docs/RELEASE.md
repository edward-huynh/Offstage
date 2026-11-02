# Releasing Offstage

**Status (2026-11-02): nothing is signed, notarized or auto-updating.** `npm run build` makes unsigned installers, which macOS Gatekeeper and Windows SmartScreen warn about. Everything below needs credentials that only the project owner has, so none of it is wired up. What is ready is listed first.

## Ready

- `npm run build` builds the local model sidecar (`offstage-llm-host`) and bundles it next to the app's executable (`apps/desktop/src-tauri/tauri.bundle.conf.json`, `scripts/prepare-sidecar.mjs`). Checked on macOS arm64: it ends up in `Offstage.app/Contents/MacOS/`.
- `apps/desktop/src-tauri/Entitlements.plist` holds `com.apple.security.device.audio-input`, and `tauri.bundle.conf.json` points the macOS bundle at it. Tauri applies it when the app is signed. Without it, a notarized (hardened runtime) app is refused the microphone. It has not been exercised yet, because no build has been signed.
- `Info.plist` carries the microphone and system audio usage descriptions macOS shows in its prompts.

## What you need to provide

| For | You need |
|---|---|
| macOS signing | Apple Developer Program membership, and a **Developer ID Application** certificate exported as `.p12` |
| macOS notarization | An app-specific password for your Apple ID and the team ID, or an App Store Connect API key |
| Windows signing | A code signing certificate (EV avoids the SmartScreen warning from the first download; OV builds reputation slowly), or Azure Trusted Signing |
| Auto-update | A key pair made with `tauri signer generate`, and somewhere to host the update files (GitHub Releases works) |

Keep every secret in the CI's secret store, never in the repository (`gitleaks` runs on every push).

## macOS: sign and notarize

Tauri signs, notarizes and staples by itself when these environment variables are set while `npm run build` runs:

| Variable | Value |
|---|---|
| `APPLE_CERTIFICATE` | The `.p12` file, base64-encoded |
| `APPLE_CERTIFICATE_PASSWORD` | Its password |
| `APPLE_SIGNING_IDENTITY` | For example `Developer ID Application: Your Name (TEAMID)` |
| `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | For notarization with an app-specific password |
| or `APPLE_API_ISSUER`, `APPLE_API_KEY`, `APPLE_API_KEY_PATH` | For notarization with an API key |

The names and behaviour are those of Tauri v2: check the [Tauri signing guide](https://v2.tauri.app/distribute/sign/macos/) for the version in `apps/desktop/package.json` before relying on them.

After the first signed build, check on a Mac that did not build it:

```bash
codesign --verify --deep --strict --verbose=2 Offstage.app
codesign -d --entitlements - Offstage.app            # audio-input must be listed
codesign -d --entitlements - Offstage.app/Contents/MacOS/offstage-llm-host
spctl -a -vv Offstage.app                             # "accepted, source=Notarized Developer ID"
```

Then run the app: the microphone prompt must appear, system audio capture must work, and a local model must load. The sidecar is a second executable inside the bundle. If it is not signed with the same identity, or the hardened runtime blocks something it needs (loading the model, Metal), local answers will fail only in the signed build. That is the thing most likely to need an extra entitlement, and it cannot be tested without a signed build.

## Windows: sign

Either a certificate in the Windows store (`bundle.windows.certificateThumbprint`, `digestAlgorithm`, `timestampUrl`), or a custom command (`bundle.windows.signCommand`) for Azure Trusted Signing or a hardware token. Sign the installer and the executables, including `offstage-llm-host.exe`, which Tauri bundles as an external binary. Put the settings in `tauri.bundle.conf.json` or a second config file passed to `tauri build --config`, not in `tauri.conf.json`, so `npm run dev` stays unaffected.

The Windows sidecar is built for the CPU only (no Vulkan or CUDA backend is selected), so local answers are slower there than on an Apple GPU. A Vulkan build exists as an option, not yet as the default: `OFFSTAGE_LLM_FEATURES=vulkan npm run build` (the script passes it to `cargo build -p offstage-llm-host --features`). It needs the [Vulkan SDK](https://vulkan.lunarg.com/) installed with `VULKAN_SDK` set, and the resulting `offstage-llm-host.exe` links `vulkan-1.dll`, which a GPU driver installs: on a computer without a Vulkan driver the sidecar would not start at all. Before making it the default, either ship two sidecars and pick one at run time, or switch to llama.cpp's dynamic backends. Neither has been tried. The speech model has the same option (`offstage-stt` feature `vulkan`) and is also CPU-only on Windows today. Nobody has built `llama.cpp` or ONNX Runtime on the Windows CI runner yet: the first CI run after the push is the first test.

## Auto-update

Not started. The pieces, in order:

1. `npm --prefix apps/desktop run tauri signer generate -- -w ~/.tauri/offstage.key`. Keep the private key and its password as CI secrets (`TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`). If it is lost, installed copies can never be updated.
2. Add `tauri-plugin-updater` to `src-tauri/Cargo.toml` and the capability `updater:default`, register it in `lib.rs`, and set `bundle.createUpdaterArtifacts` to `true` in the bundle config.
3. In `tauri.conf.json`, `plugins.updater` takes the **public key** and the `endpoints` (for example a `latest.json` on GitHub Releases). A wrong or missing public key makes the plugin fail at startup, so add it only together with the real key.
4. A CI job on version tags builds both installers, signs them, and uploads the installers, their `.sig` files and `latest.json` to a release.
5. A screen or a toast that checks for an update, shows the version and installs it. The updater replaces the whole app, so the sidecar is updated with it. Stop the sidecar before installing.

## Before a release

- [ ] CI is green on macOS and Windows, including the bundle jobs.
- [ ] Screen-share hiding checked for Meet, Teams, Zoom and OBS on both systems, and [COMPATIBILITY.md](COMPATIBILITY.md) filled in.
- [ ] The first-run walkthrough, the speech model download, a call with captions, and the assistant (cloud, local, fallback) tried on a clean user account of each system.
- [ ] Signed and notarized build checked as above.
- [ ] Version bumped in `Cargo.toml` (workspace) and `tauri.conf.json`, and the changelog written.
- [ ] README status line updated.
