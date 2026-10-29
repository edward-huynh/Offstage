# Architecture

```
apps/desktop/        Tauri v2 shell + React overlay UI
crates/core/         config, workspaces, AppEvent (Rust → UI)
crates/audio/        mic + system loopback capture, VAD          (Phase 2)
crates/stt/          whisper.cpp speech-to-text                  (Phase 2)
crates/llm/          local llama.cpp + OpenAI-compatible client  (Phase 3)
crates/rag/          ingest, chunk, embed, hybrid search         (Phase 4)
```

## Runtime pipeline

```
loopback/mic ─► VAD ─► STT ─► question detection ─► RAG retrieve ─► LLM ─► overlay
```

- Each stage runs on its own task, connected by channels; a new question cancels the answer in progress.
- Rust emits `AppEvent`s; the UI only renders. API calls never happen in the webview, so keys never reach it.

## Providers

All cloud providers go through one OpenAI-compatible client configured by `ProviderConfig`
(base URL, API style, model, `key_ref`). Built-in presets: OpenAI, OpenCode Zen, OpenCode Go.
Users can add any custom OpenAI-compatible endpoint.

## Storage

- Settings: JSON in the app config dir.
- Secrets: OS keychain via `keyring`; `key_ref` points at them.
- RAG: one SQLite file per workspace (`sqlite-vec` + FTS5 with `remove_diacritics 0`).
