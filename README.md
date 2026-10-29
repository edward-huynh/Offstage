# Offstage

Local-first desktop overlay: a teleprompter that stays out of screen shares, plus (on the roadmap) live transcription, question detection and answers grounded in your own documents.

Trợ lý overlay chạy local: đọc kịch bản, không hiện khi chia sẻ màn hình. Các tính năng nghe câu hỏi, RAG và LLM đang trong lộ trình.

## Status

Phase 0 (scaffold). Working today: overlay window with content protection, tray menu, teleprompter with keyboard controls. See [docs/ROADMAP.md](docs/ROADMAP.md).

## Quick start

Prerequisites: Rust stable, Node 22, and the [Tauri v2 system prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS.

```bash
npm run setup     # install frontend deps
npm run dev       # run the desktop app
npm run check     # fmt + clippy + tests + typecheck (same as CI)
npm run build     # installers in target/release/bundle
```

### Teleprompter keys

| Key | Action |
|---|---|
| Space | Play / pause |
| ↑ / ↓ | Speed |
| + / − | Font size |
| M | Mirror |
| E | Edit script (Esc to finish) |
| Home | Back to top |
| Esc | Hide (bring back from the tray) |

## Screen-share exclusion

| OS | Mechanism | Notes |
|---|---|---|
| Windows 10 2004+ / 11 | `WDA_EXCLUDEFROMCAPTURE` | Window is removed from capture |
| macOS | `NSWindow.sharingType = .none` | Apps using ScreenCaptureKit on macOS 15+ may still capture it; see the compatibility table in docs |

Physical cameras and capture cards can always see the screen.

## Privacy

- Local by default. Cloud providers are opt-in per workspace and shown in the UI when active.
- API keys live in the OS keychain, never in config files or the repo.
- Audio is never written to disk.

## Responsible use

Offstage is a general-purpose tool. You are responsible for following the rules of the meetings, interviews, exams and organisations you use it in, and for any recording-consent and personal-data laws that apply to you. Many employers and exam bodies prohibit undisclosed AI assistance.

## License

MIT. Models are downloaded separately and keep their own licenses.
