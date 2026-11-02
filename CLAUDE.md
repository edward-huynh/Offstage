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
| Lightweight | App binary about 50 MB (it was under 20 MB until ONNX Runtime was linked in for embeddings, see section 8 and the decision log), idle RAM under 150 MB. Models are downloaded on first run, never bundled. |
| Automated | Auto-pick models by hardware, auto-index documents, auto-fallback between providers. Avoid manual steps for the user. |
| Extensible by config | New providers and workspace templates are config/presets, not new code paths. |
| Private | No audio written to disk. API keys only in the OS keychain. API calls only from Rust, never from the webview. |

## 3. Tech stack

- **Shell**: Tauri v2 (Rust) with a React 19 + TypeScript (strict) + Vite overlay UI.
- **UI**: Tailwind CSS v4 with beUI (MIT, copied-in motion components) in `apps/desktop/src/components/motion`; the overlay keeps its own small CSS so it can stay transparent and frameless.
- **Audio**: `cpal` 0.18 for the microphone and for loopback (a Core Audio tap on macOS 14.6+, WASAPI loopback on Windows), `rubato` to resample to 16 kHz. ScreenCaptureKit is the planned fallback for older macOS. VAD is an energy detector for now, Silero (ONNX) is still to come.
- **STT**: `whisper-rs` (whisper.cpp), model `small` q5 by default, PhoWhisper option for Vietnamese.
- **LLM local**: `llama-cpp-2`, 3–4B instruct models in Q4 (Qwen3 4B, Gemma 3 4B class). Check Hugging Face for current versions.
- **LLM cloud**: one OpenAI-compatible client (`reqwest` + SSE).
- **RAG**: `fastembed-rs` (multilingual-e5-small default, bge-m3 optional), SQLite + FTS5 with the vectors stored as blobs and compared in Rust (no `sqlite-vec`), hybrid search with RRF.
- **Secrets**: `keyring` crate.
- **CI**: GitHub Actions (gitleaks, fmt/clippy/test, Windows + macOS bundles).

## 4. Repository layout

```
apps/desktop/                 Tauri app (`src-tauri/tauri.bundle.conf.json` adds the sidecar as `externalBin` for `npm run build`)
  src/                        React UI (main/, overlay/, hooks, tauri.ts)
    components/motion/        vendored beUI components (button, card, input, radio...)
    lib/                      vendored beUI helpers (ease, hooks, cn)
    styles/theme.css          Tailwind v4 entry and the app's design tokens
  src-tauri/                  Rust shell: window, tray, commands
    tauri.conf.json           overlay window: transparent, alwaysOnTop, contentProtected
    capabilities/default.json window permissions
crates/core/                  OverlayConfig, Workspace, AppEvent (Rust → UI), Speaker
crates/audio/                 AudioSource trait, CpalSource (mic + loopback), resample to 16 kHz mono, Vad + Segmenter
crates/stt/                   SpeechToText trait, WhisperStt behind the `whisper` feature (`metal`, `cuda`, `vulkan`)
crates/models/                ModelSpec presets and a resumable, checksum-verified downloader
crates/pipeline/              AudioSource -> Segmenter -> SpeechToText -> AppEvent, one lane per source
crates/llm/                   LlmProvider trait, ProviderConfig, presets (OpenAI, OpenCode Zen/Go), the client of the local model sidecar, and the router (local_only, fallback, first-token timeout)
crates/llm-proto/             JSON-lines protocol between the app and the sidecar
apps/llm-host/                `offstage-llm-host`: runs llama.cpp (Qwen3 GGUF) in its own process
crates/rag/                   text extraction (PDF, DOCX, MD, TXT), chunker, FTS5 index, vector storage and hybrid search (BM25 + embeddings, RRF) over the user's documents
crates/embed/                 TextEmbedder trait, multilingual-e5-small on ONNX Runtime behind the `onnx` feature
scripts/prepare-sidecar.mjs     builds the sidecar and copies it where Tauri bundles it from
docs/                         ARCHITECTURE.md, ROADMAP.md
.github/workflows/ci.yml      CI pipeline
```

Crates must stay independent and testable without Tauri. Only `apps/desktop/src-tauri` depends on Tauri.

## 5. Commands

```bash
npm run setup   # install frontend deps (the Rust side also needs cmake and a C++ toolchain: `brew install cmake`)
npm run dev     # build the local model sidecar, then run the desktop app (tauri dev)
npm run ui      # frontend only in a browser, for styling work
npm run check   # cargo fmt --check, clippy -D warnings, cargo test, tsc
npm run build   # build the sidecar, then installers in target/release/bundle (the sidecar ends up next to the app's executable)
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

- Each source gets a lane of two threads (segmenter, then speech-to-text) joined by bounded `std::sync::mpsc` channels. The capture callback and `transcribe` both block, so plain threads fit better than tokio tasks here. Later stages (LLM streaming) can bridge to tokio.
- The speech engine (about 190 MB) is loaded when listening starts and released when the last source stops, to keep idle RAM low.
- A newly detected question cancels the answer currently being generated.
- Rust emits `AppEvent` (see `crates/core`) to the UI via Tauri events: `transcript`, `question_detected`, `answer_token`, `answer_done`, `error`, `model_progress`. `AppEvent::name()` gives the event name.
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
  - OpenCode Zen (pay per use): `https://opencode.ai/zen/v1`. The native API differs per model (GPT via `/responses`, Claude via `/messages`), but the OpenCode docs (read 2026-11-02) say every text model also works through `/chat/completions`, so the client only speaks that. The docs now list base URLs under `https://opencode.ai/inference/...`, while `/zen/v1` and `/zen/go/v1` still answer `GET /models`.
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

## 8. Current status (Phase 2 done, Phase 3 cloud assistant and Phase 4 in progress, not yet released)

Done:
- Repo layout, workspace, 5 crates with traits and types.
- Overlay window: transparent, frameless, always on top, content-protected, tray (show/hide, quit).
- Teleprompter UI: auto-scroll (rAF, frame-rate independent), Space play/pause, ↑↓ speed, +/− font, M mirror, E edit, Home top, Esc hide.
- Provider presets with tests, Markdown chunker with tests, CI, docs, MIT license.
- Phase 1 features: persisted settings, global hotkeys, click-through, window position, multiple scripts, old Windows warning.
- Phase 2 code: capture (`cpal`), resample, VAD + segmenter, whisper engine, model downloader, pipeline, live captions panel (T, 1, 2), macOS permission flow (microphone is asked for up front, system audio via a guided retry).
- Phase 3 first slice (cloud only at first): question detection from the remote captions, OpenAI-compatible streaming client, API key in the OS keychain, assistant Settings section, answer panel on the overlay. Off by default; the choice lives in the `assistant_config` key. The prompt ends with a language line ("Answer in English." / "Trả lời bằng tiếng Việt.") taken from the language Whisper reported for the question, because small models answered English questions in Vietnamese otherwise (`docs/LLM-BENCH.md`). The local model and the router came after (below).
- Whole short documents (2026-11-02): when search finds nothing and all the documents together are at most 8,000 characters (3,000 in local-only mode), all of them are sent (`ChunkStore::everything_if_small`, `gather` in `assistant.rs`). Reason, measured with e5 and the real CV fixture: an English question against a Vietnamese CV of one chunk scores 0.76 to 0.79, no better than "what is the weather like today" (0.78), and shares no word with it, so "Where did you study?" found nothing. Side effect: an off-topic question now gets "Không có trong tài liệu." instead of a general answer while the documents are short. The same test found Qwen3 4B answering "University of Science, Ho Chi Minh City" for a CV that says "Đại học Bách khoa Hà Nội" when told only to answer in English; the English language line now adds "Keep names of schools, companies and places exactly as written in the documents" when there are excerpts, and the system prompt has a rule to keep proper names as written. Qwen3 1.7B still answered in Vietnamese to an English question in one run.
- Phase 3 second slice (local model and router): `llama.cpp` cannot share a process with `whisper.cpp` (see the risks table), so the model runs in `offstage-llm-host`, a separate binary the app starts when an answer needs it (or when listening starts in local-only mode) and stops when listening ends. They talk in JSON lines on stdin/stdout (`offstage-llm-proto`), so there is no socket and no firewall prompt. Models: Qwen3 4B (default) and 1.7B, Q4_K_M, downloaded from Settings with checksum. `Router` (`crates/llm/src/router.rs`): `local_only` never asks the provider and never reads its key; otherwise the provider is asked first and, if it errors or writes no token within 5 s, the local model answers instead (only before any token is shown; the overlay says so). The excerpt budget drops to 3 chunks and 2,000 characters in local-only mode. Measured with the packaged sidecar and Qwen3 4B on an M1 Pro: load 9 s cold, first token 1.5 s with a grounded prompt, Vietnamese bullets in the first person.
- Phase 4 first slice (lexical RAG): documents added in Knowledge are read in the background (PDF with `pdf-extract`, DOCX from `word/document.xml`, MD, TXT), cut into chunks and stored with an FTS5 index (migration 2, `unicode61 remove_diacritics 0`). On every detected question the assistant searches it (BM25, a relevance gate based on neighbouring word pairs and rare names, at most 4 excerpts and 4,000 characters) and sends the excerpts with the question using the grounded prompt of `docs/LLM-BENCH.md` variant D. The sources are listed under the answer on the overlay, and the Knowledge screen shows per document whether the AI could read it.
- Phase 4 second slice (embeddings): once the user downloads multilingual-e5-small (int8, about 135 MB, a button on the Knowledge screen, five checksum-verified files through `offstage-models`), every chunk gets a 384-dimension vector (stored as a blob, compared in code, no sqlite-vec) and a question is searched both ways and fused with RRF. The best chunk of each ranking is always sent. The model is loaded while listening with the assistant on, and when chunks need vectors, and released after. Without the model search is by words only, as before. Cosine similarity cannot tell an off-topic question about the same field from an answerable one (0.87 for "explain Terraform" against Node.js notes, 0.83 for a correct English question), so the floor is only 0.83 and the grounded prompt is what makes the model say "Không có trong tài liệu." (`crates/rag/src/semantic.rs`).

Verified: `npm run check` and CI (macOS and Windows, bundles uploaded; whisper builds on both, run on 219f5b5); the overlay opens on macOS; the speech pipeline end to end on Metal with synthetic speech, latency and accuracy in `docs/STT-EVAL.md` (STT 0.4 to 0.9 s, Vietnamese WER 13.6% on clean synthetic speech); the macOS bundle carries the microphone and system audio usage descriptions and requires macOS 13.0; release binary 15.6 MiB at Phase 2 (18.1 MiB with the cloud assistant).

**Binary size, accepted:** with embeddings the macOS release binary is 49.8 MiB (38.4 MiB stripped) instead of 18.1 MiB, because ONNX Runtime is linked statically (`ort`'s prebuilt `libonnxruntime.a` is 80 MB before linking). Decided on 2026-11-02 to keep it rather than download the runtime on first use (`ort` feature `load-dynamic`) or switch to a pure-Rust engine (`candle`). Cheap ways to shrink it later: `strip = true` and LTO in the release profile. Measured on macOS arm64 with no `strip` or LTO set.

Not yet verified (local model): the whole flow inside the Tauri app (download, listening in local-only mode, the settings card, fallback after a real provider failure); Windows (the sidecar is built CPU-only there, no Vulkan or CUDA, and nobody has built llama.cpp on the Windows CI runner yet); the first fallback answer in a session waits for the model to load (about 9 s cold).

Not yet verified: screen-share exclusion in Meet, Teams, Zoom and OBS (`docs/COMPATIBILITY.md` is a checklist with no results yet); the microphone and loopback sources on real hardware and the permission prompts and settings links (the system audio link points at the Screen & System Audio Recording page and is untested); the captions panel inside the Tauri window (only seen in a browser); accuracy on real human speech; Phase 2 behaviour on Windows beyond compiling; the assistant against a real provider (first-token latency, the keychain prompt on an unsigned build, the answer panel in the Tauri window).

## 9. Roadmap

Estimates assume one developer part-time.

### Phase 0: Foundation (finish)
- [x] `npm run dev` compiles and opens the overlay on macOS
- [ ] Confirm overlay is hidden when sharing the full screen in Google Meet (Chrome), Teams, Zoom, OBS; record results in `docs/COMPATIBILITY.md`
- [x] First CI run green, Windows and macOS bundles downloadable from Actions (green on 27e2e3c and again on 219f5b5, which includes whisper: all five jobs passed, both bundles uploaded)
- [x] **Spike**: capture system audio on macOS (ScreenCaptureKit audio or Core Audio taps) in a throwaway binary under `spikes/`; document permissions needed and latency (see `docs/AUDIO-SPIKE.md`)

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
- [x] Local model: `llama-cpp-2` in the sidecar `offstage-llm-host` (not in the app process, see risks), streaming tokens, loaded while listening and released after (`crates/llm/src/local.rs`)
- [x] `crates/llm/openai_compat.rs`: SSE streaming, both error shapes, `list_models`, `test_connection`, cancellation (tested against a mock server; not yet against a real provider)
- [x] Keychain storage via `keyring`; env var fallback (macOS and Windows only; the keychain path is not yet exercised on a real machine)
- [~] Router: `local_only`, fallback and first-token timeout done (`crates/llm/src/router.rs`); per-workspace provider waits for workspaces
- [x] Question detection: heuristic first (`?`, Vietnamese question words: gì, sao, tại sao, như thế nào, bao nhiêu, có … không, ở đâu, khi nào), LLM classification optional (heuristic done in `offstage_core::question`)
- [x] Settings UI: provider picker, paste key, Test connection, model dropdown from `/models`
- [x] Cloud/local indicator (☁️ / 💻) on the overlay, and a note under an answer the local model wrote after the provider failed
- **Done when**: first token under 2 s after the question ends (GPU or cloud).

### Phase 4: RAG (2–3 weeks)
- [ ] Workspaces: create, rename, delete, context note, provider, `local_only` (documents are one global set for now, `Workspace` is unused)
- [~] Ingest PDF (`pdf-extract`), DOCX (XML), MD, TXT with background indexing and per-document status (no drag-and-drop, no progress bar; scanned PDFs are reported as unreadable)
- [x] Extend chunker to non-Markdown text
- [x] Embeddings via `fastembed-rs` (multilingual-e5-small int8, user-defined model files); storage in SQLite with FTS5; vectors are blobs compared in Rust instead of `sqlite-vec` (bge-m3 option not done)
- [x] Hybrid search + RRF; prompt with sources
- [ ] Folder watch (`notify`) with content-hash incremental re-index
- [~] Evaluation set: 50 questions with expected sources under `crates/rag/tests/eval/`, runnable as a test (10 questions over `samples/knowledge`: words only in `crates/rag/tests/samples.rs`, and with the real model in `crates/rag/tests/semantic.rs`, ignored by default, 10 of 10 found within four excerpts)
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
- `/responses` and `/messages` API styles for Zen GPT/Claude models. Probably not needed: the OpenCode docs (read 2026-11-02) say every text model also answers through `chat/completions`, which Console translates. Not tried with a key; if a GPT or Claude model fails, this is the fix
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
| Weak Vietnamese STT on small models | PhoWhisper-small q5_1 (converted from the official weights, reproducible with `spikes/stt-bench/convert-phowhisper.sh`) cut Vietnamese WER from 14.0% to 1.7% on synthetic speech, but it is Vietnamese-only and needs a place to host the converted file (see `docs/STT-EVAL.md`). Optional cloud STT stays a separate toggle |
| High latency on CPU-only machines | Auto-pick smaller models, suggest a cloud provider |
| `whisper-rs` and `llama-cpp-2` cannot share one process | Both statically embed their own `ggml`. Tried 2026-11-02 on macOS (whisper-rs 0.16 with llama-cpp-2 0.1.160, both `metal`): it links without errors, whisper runs, then loading a Qwen3 context aborts with `GGML_ASSERT(a->op == GGML_OP_FLASH_ATTN_EXT)` because the symbols of the two ggml versions mix. The local LLM must run outside the app process (a sidecar) or use an engine without ggml (candle) |
| Model licenses (e.g. Gemma terms) | Never bundle weights, show license at download |
| Misuse and project reputation | README responsible-use section, promote legitimate use cases |

## 12. Decision log

| Decision | Why |
|---|---|
| Tauri over Electron | ~15 MB vs ~150 MB app, Rust backend for audio/ML |
| Local-first, cloud opt-in | Privacy, works offline, no running cost |
| ONNX Runtime linked statically (binary 18 → 50 MB) | The embedding model needs a runtime, and downloading one per OS on first use adds a second download, checksums for archives and signing questions. Accepted over the 20 MB target; `load-dynamic` stays the way back |
| One OpenAI-compatible client | OpenAI, OpenCode and most gateways speak it; new providers become config |
| SQLite + FTS5, vectors as blobs | Single file per workspace, no server, hybrid search catches names and terms. Brute-force cosine over a few thousand chunks takes milliseconds, so no `sqlite-vec` extension is loaded |
| Keys in OS keychain, calls from Rust only | Keys never reach the webview or disk |
| Local LLM as a sidecar process, not in the app | whisper.cpp and llama.cpp each embed their own `ggml` and cannot share a process. A sidecar also gives the memory back by exiting, and needs no socket. Costs: a second binary to bundle (+10.6 MiB) and a build step (`scripts/prepare-sidecar.mjs`) |
| Stub crates from day one | Clear boundaries, independent tests, reusable outside the app |
| Whisper runs with timestamps off | The app only uses text. Timestamps cost decoding steps, and models fine-tuned without them (PhoWhisper) emit stray tokens when asked for them |
| PhoWhisper is an optional model, not the default | It is Vietnamese-only and writes lowercase text without punctuation, so `small` stays the multilingual default. Question detection must rely on Vietnamese question words, not on `?` |
