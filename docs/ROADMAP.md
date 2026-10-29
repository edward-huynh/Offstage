# Roadmap

| Phase | Scope | Done when |
|---|---|---|
| 0. Foundation | Repo, crates, CI on 3 OSes, gitleaks, license | Every PR builds Windows/macOS installers |
| 1. Overlay + teleprompter | Global hotkeys, settings persistence, click-through, multiple scripts, import .txt/.md | Hidden in Meet, Teams, Zoom, OBS on Windows; macOS results documented |
| 2. Audio + STT | Loopback + mic, Silero VAD, whisper-rs, live transcript | STT under 1 s on GPU, usable Vietnamese |
| 3. LLM + providers | llama.cpp local, OpenAI-compatible client, keychain, model listing, fallback | First token under 2 s after the question ends |
| 4. RAG | Workspaces, PDF/DOCX/MD import, hybrid search, cited answers, folder watch | ≥ 80% grounded answers on a 50-question test set |
| 5. v1.0 | Onboarding, model downloader, code signing, auto-update, vi/en UI | New user productive within 5 minutes |

## Later

- Voice-follow scrolling
- Answer modes: short, detailed, suggested follow-up questions
- Session summary exported to Markdown
- Workspace templates: interviewer, interview practice, presentation, customer calls
- Practice mode: the app asks questions from your documents and reviews your spoken answers
- Live translation (vi ↔ en)
- Plugins for providers and document sources
- Linux support

## Risks

| Risk | Mitigation |
|---|---|
| macOS 15+ ScreenCaptureKit ignores `sharingType` | Test early, publish a compatibility table, document single-window sharing |
| Weak Vietnamese STT on small models | PhoWhisper option, optional cloud STT |
| High latency on CPU-only machines | Auto-pick smaller models, suggest a cloud provider |
| Model licenses | Never bundle weights; show license at download |
