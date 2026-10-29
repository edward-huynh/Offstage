# Offstage

Project brief, conventions and roadmap for Claude Code. Read this first in every session.

## 1. What Offstage is

A local-first desktop overlay that sits on top of every app and stays out of screen shares.

- **Teleprompter**: reads a script with smooth auto-scroll, controlled from the keyboard.
- **Live assistant** (roadmap): listens to the call, detects questions, and streams short answers grounded in the user's own documents (RAG).
- **Model choice**: a small local model by default; OpenAI, OpenCode Zen/Go or any OpenAI-compatible endpoint as an opt-in.

Open source (MIT), general purpose. Use cases we promote: presentations, interview practice, interviewers with a JD/CV/rubric, customer and sales calls.

## 2. Principles (apply to every change)

| Principle | Concretely |
|---|---|
| Local-first | Works fully offline by default. Cloud is opt-in per workspace and visibly indicated in the overlay. |
| Lightweight | App binary under 20 MB, idle RAM under 150 MB. Models are downloaded on first run, never bundled. |
| Automated | Auto-pick models by hardware, auto-index documents, auto-fallback between providers. Avoid manual steps for the user. |
| Extensible by config | New providers and workspace templates are config/presets, not new code paths. |
| Private | No audio written to disk. API keys only in the OS keychain. API calls only from Rust, never from the webview. |

## 3. Tech stack

- **Shell**: Tauri v2 (Rust) with a React 18 + TypeScript (strict) + Vite overlay UI.
- **Audio**: `cpal` / `wasapi` (Windows loopback), ScreenCaptureKit or Core Audio taps (macOS loopback), Silero VAD (ONNX).
- **STT**: `whisper-rs` (whisper.cpp), model `small` q5 by default, PhoWhisper option for Vietnamese.
- **LLM local**: `llama-cpp-2`, 3–4B instruct models in Q4 (Qwen3 4B, Gemma 3 4B class). Check Hugging Face for current versions.
- **LLM cloud**: one OpenAI-compatible client (`reqwest` + SSE).
- **RAG**: `fastembed-rs` (multilingual-e5-small default, bge-m3 optional), SQLite + `sqlite-vec` + FTS5, hybrid search with RRF.
- **Secrets**: `keyring` crate.
- **CI**: GitHub Actions (gitleaks, fmt/clippy/test, Windows + macOS bundles).

## 4. Repository layout

```
apps/desktop/                 Tauri app
  src/                        React overlay UI (App.tsx, useAutoScroll.ts, tauri.ts)
  src-tauri/                  Rust shell: window, tray, commands
    tauri.conf.json           overlay window: transparent, alwaysOnTop, contentProtected
    capabilities/default.json window permissions
crates/core/                  OverlayConfig, Workspace, AppEvent (Rust → UI), Speaker
crates/audio/                 AudioSource trait, AudioFrame (16 kHz mono)
crates/stt/                   SpeechToText trait
crates/llm/                   LlmProvider trait, ProviderConfig, presets (OpenAI, OpenCode Zen/Go)
crates/rag/                   Chunk, SearchHit, Markdown chunker (implemented + tested)
docs/                         ARCHITECTURE.md, ROADMAP.md
.github/workflows/ci.yml      CI pipeline
```

Crates must stay independent and testable without Tauri. Only `apps/desktop/src-tauri` depends on Tauri.

## 5. Commands

```bash
npm run setup   # install frontend deps
npm run dev     # run the desktop app (tauri dev)
npm run ui      # frontend only in a browser, for styling work
npm run check   # cargo fmt --check, clippy -D warnings, cargo test, tsc
npm run build   # installers in target/release/bundle
```

**Always run `npm run check` before committing.** CI runs the same steps.

## 6. Conventions

- Rust: `cargo fmt`, zero clippy warnings, errors via `thiserror`, no `unwrap()` outside tests.
- TypeScript: strict mode, no unused code, UI only renders state and sends commands. No business logic or network calls in the webview.
- Commits: Conventional Commits (`feat:`, `fix:`, `docs:`, `chore:`, `refactor:`, `test:`), small and focused.
- Tests: every crate function with logic gets unit tests. Use fixtures in `crates/<name>/tests/fixtures/`.
- UI copy: Vietnamese first, English second (i18n arrives in Phase 5). Sentence case, plain verbs.
- Never commit: API keys, `.env`, model weights (`*.gguf`, `*.bin`, `*.onnx`), audio recordings.
- When a roadmap task is done, tick it in section 9 and in `docs/ROADMAP.md`.

## 7. Architecture

### Runtime pipeline

```
system loopback (Remote) ─┐
microphone (Me)          ─┴─► VAD ─► STT ─► question detection ─► RAG retrieve ─► LLM ─► overlay
```

- Each stage runs on its own tokio task, connected by `tokio::mpsc` channels.
- A newly detected question cancels the answer currently being generated.
- Rust emits `AppEvent` (see `crates/core`) to the UI via Tauri events: `transcript`, `question_detected`, `answer_token`, `answer_done`, `error`.
- Speaker separation comes for free from the channel: mic = `Me`, loopback = `Remote`. Question detection only looks at `Remote` by default.

### Screen-share exclusion

| OS | Mechanism | Status |
|---|---|---|
| Windows 10 2004+ / 11 | `WDA_EXCLUDEFROMCAPTURE` via Tauri `contentProtected` | Expected to work with Meet, Teams, Zoom, OBS |
| Older Windows 10 | `WDA_MONITOR` fallback shows a black box | Detect OS version, warn the user |
| macOS | `NSWindow.sharingType = .none` | macOS 15+ ScreenCaptureKit-based capture may still see the window. Test and document. |

`contentProtected` is set both in `tauri.conf.json` and at runtime in `setup()`. Keep both.

### Providers

- `ProviderConfig { id, label, base_url, api_style, model, key_ref }`. `key_ref` is `keychain:<name>` or `env:<VAR>`. The key itself never appears in config or logs.
- `ApiStyle`: `ChatCompletions` (default), `Responses`, `AnthropicMessages`.
- Built-in presets:
  - OpenAI: `https://api.openai.com/v1`
  - OpenCode Zen (pay per use): `https://opencode.ai/zen/v1`. Most models via `/chat/completions`; GPT models via `/responses`, Claude models via `/messages`.
  - OpenCode Go (subscription): `https://opencode.ai/zen/go/v1`
  - Zen and Go share one API key (`keychain:opencode`).
- OpenCode returns a custom error shape (`{"type":"error","error":{"type":"AuthError","message":...}}`). The error parser must handle both this and the standard OpenAI shape.
- Custom endpoint: any OpenAI-compatible URL (OpenRouter, Ollama, LM Studio, Groq...).
- Router: per-workspace provider, `local_only` flag blocks all cloud calls, automatic fallback cloud → local on error/timeout/offline.

### RAG

- One SQLite file per workspace: `chunks`, `chunks_fts` (FTS5, `tokenize='unicode61 remove_diacritics 0'` so Vietnamese diacritics are kept), `chunks_vec` (`vec0`).
- Ingest: PDF, DOCX, MD, TXT → chunk (~1600 chars, split on headings/paragraphs, keep `doc` + `section`) → embed → store. Content hash per file so only changed files are re-indexed.
- Query: embed question (e5 needs `query:` / `passage:` prefixes) → BM25 + vector → RRF fusion → top 4–6 chunks, max ~2000 tokens of context.
- Prompt: answer only from documents, 3–5 short bullets, same language as the question, say "Không có trong tài liệu." when unsupported, show sources under the answer.

## 8. Current status (Phase 0)

Done:
- Repo layout, workspace, 5 crates with traits and types.
- Overlay window: transparent, frameless, always on top, content-protected, tray (show/hide, quit).
- Teleprompter UI: auto-scroll (rAF, frame-rate independent), Space play/pause, ↑↓ speed, +/− font, M mirror, E edit, Home top, Esc hide.
- Provider presets with tests, Markdown chunker with tests, CI, docs, MIT license.

Verified: frontend build; fmt, clippy and tests on the library crates; `apps/desktop/src-tauri` compiles and the overlay opens on macOS via `npm run dev`; `npm run check` passes.

Not yet verified: screen-share exclusion in Meet, Teams, Zoom and OBS; CI has never run (no git repo yet).

## 9. Roadmap

Estimates assume one developer part-time.

### Phase 0: Foundation (finish)
- [x] `npm run dev` compiles and opens the overlay on macOS
- [ ] Confirm overlay is hidden when sharing the full screen in Google Meet (Chrome), Teams, Zoom, OBS; record results in `docs/COMPATIBILITY.md`
- [ ] First CI run green, Windows and macOS bundles downloadable from Actions
- [ ] **Spike**: capture system audio on macOS (ScreenCaptureKit audio or Core Audio taps) in a throwaway binary under `spikes/`; document permissions needed and latency

### Phase 1: Overlay + teleprompter (2 weeks)
- [ ] Global hotkeys via `tauri-plugin-global-shortcut` (work when overlay is not focused): play/pause, speed, show/hide, click-through toggle
- [ ] Click-through mode (`set_ignore_cursor_events`) with a visible indicator
- [ ] Persist `OverlayConfig` and window position/size (`tauri-plugin-store`)
- [ ] Multiple scripts: list, create, rename, delete; import `.txt` / `.md`
- [ ] Opacity control in UI
- [ ] Reading line position configurable
- [ ] Detect old Windows 10 builds and warn about `WDA_MONITOR`
- **Done when**: overlay hidden in Meet/Teams/Zoom/OBS on Windows; macOS results documented; settings survive restart.

### Phase 2: Audio + STT (2–3 weeks, highest risk)
- [ ] `crates/audio`: Windows WASAPI loopback, macOS loopback (from spike), microphone; resample to 16 kHz mono
- [ ] Silero VAD segmentation (~400 ms silence = end of utterance)
- [ ] `crates/stt`: whisper-rs with Metal (macOS) / CUDA or Vulkan (Windows), model selectable
- [ ] Model downloader: first-run download with progress, checksum verification, stored in app data dir
- [ ] Live transcript panel in the overlay, `Me` / `Remote` labelled
- [ ] macOS onboarding for Screen Recording / Microphone permissions
- **Done when**: STT latency under 1 s on GPU; Vietnamese transcript readable.

### Phase 3: LLM + providers (2 weeks)
- [ ] `crates/llm/local.rs`: llama-cpp-2, model kept in memory, streaming tokens
- [ ] `crates/llm/openai_compat.rs`: SSE streaming, both error shapes, `list_models`, `test_connection`, cancellation
- [ ] Keychain storage via `keyring`; env var fallback
- [ ] Router: per-workspace provider, `local_only`, fallback, timeouts
- [ ] Question detection: heuristic first (`?`, Vietnamese question words: gì, sao, tại sao, như thế nào, bao nhiêu, có … không, ở đâu, khi nào), LLM classification optional
- [ ] Settings UI: provider picker, paste key, Test connection, model dropdown from `/models`
- [ ] Cloud/local indicator (☁️ / 💻) on the overlay
- **Done when**: first token under 2 s after the question ends (GPU or cloud).

### Phase 4: RAG (2–3 weeks)
- [ ] Workspaces: create, rename, delete, context note, provider, `local_only`
- [ ] Ingest PDF (`pdf-extract` / `pdfium-render`), DOCX (XML), MD, TXT; drag-and-drop with background progress
- [ ] Extend chunker to non-Markdown text
- [ ] Embeddings via `fastembed-rs`; storage via `sqlite-vec` + FTS5
- [ ] Hybrid search + RRF; prompt with sources
- [ ] Folder watch (`notify`) with content-hash incremental re-index
- [ ] Evaluation set: 50 questions with expected sources under `crates/rag/tests/eval/`, runnable as a test
- **Done when**: ≥ 80% answers cite the correct source on the eval set.

### Phase 5: v1.0 release (2 weeks)
- [ ] Onboarding flow (permissions, model download, first script)
- [ ] Hardware detection to auto-pick model sizes
- [ ] Code signing (Windows, macOS notarization) and auto-update (`tauri-plugin-updater`)
- [ ] i18n vi/en
- [ ] User docs and landing README with GIFs
- **Done when**: a new user is productive within 5 minutes of install.

### Later (v1.x / v2)
- Voice-follow scrolling (teleprompter follows the reader's speech)
- Answer modes: short / detailed / suggested follow-up questions (hotkey)
- Session summary exported to Markdown
- Workspace templates: interviewer (JD, CV, rubric, follow-up suggestions), interview practice, presentation, customer calls
- Practice mode: app asks questions from the documents, user answers by voice, app reviews the answer
- Token cost tracking for cloud providers
- Reranker (bge-reranker-v2-m3), OCR for scanned PDFs
- `/responses` and `/messages` API styles for Zen GPT/Claude models
- Live translation vi ↔ en
- Plugins for providers and document sources (Notion, Google Drive)
- Linux support (Wayland makes capture exclusion hard)

## 10. Targets

| Metric | Target |
|---|---|
| Question end → first answer token | < 2 s (GPU or cloud), < 4 s (CPU + cloud LLM) |
| RAM | < 150 MB idle, < 5 GB full local pipeline |
| RAG grounding | ≥ 80% correct sources on eval set |
| Install → first use | < 5 minutes |

## 11. Risks

| Risk | Mitigation |
|---|---|
| macOS 15+ ScreenCaptureKit ignores `sharingType` | Test early, publish compatibility table, document single-window sharing |
| Weak Vietnamese STT on small models | PhoWhisper option, optional cloud STT as a separate toggle |
| High latency on CPU-only machines | Auto-pick smaller models, suggest a cloud provider |
| Model licenses (e.g. Gemma terms) | Never bundle weights, show license at download |
| Misuse and project reputation | README responsible-use section, promote legitimate use cases |

## 12. Decision log

| Decision | Why |
|---|---|
| Tauri over Electron | ~15 MB vs ~150 MB app, Rust backend for audio/ML |
| Local-first, cloud opt-in | Privacy, works offline, no running cost |
| One OpenAI-compatible client | OpenAI, OpenCode and most gateways speak it; new providers become config |
| SQLite + sqlite-vec + FTS5 | Single file per workspace, no server, hybrid search catches names and terms |
| Keys in OS keychain, calls from Rust only | Keys never reach the webview or disk |
| Stub crates from day one | Clear boundaries, independent tests, reusable outside the app |
