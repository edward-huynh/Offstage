# Offstage

A local-first desktop overlay that sits on top of every app and stays out of your screen shares.

- **Teleprompter** with smooth auto-scroll, keyboard controls and global shortcuts.
- **Live captions** of the call: the other side's voice from the system audio, yours from the microphone, recognized on your computer.
- **AI assistant** that detects the other side's questions and suggests a few short bullet points, written from your own documents (CV, job description, notes). It can run entirely on your computer.
- **Workspaces**: one per situation (an interview, a sales call), each with its own documents, a note for the AI and its own privacy choices.

Vietnamese and English interface. Open source, MIT.

> **Status:** not released yet. It builds and runs on macOS and Windows, but screen-share hiding has not been checked against Meet, Teams, Zoom or OBS ([docs/COMPATIBILITY.md](docs/COMPATIBILITY.md)), and the installers are not signed or notarized ([docs/RELEASE.md](docs/RELEASE.md)).

*Tiếng Việt:* Lớp phủ trong suốt chạy trên máy bạn: đọc kịch bản, hiện phụ đề cuộc gọi và gợi ý câu trả lời từ chính tài liệu của bạn, mà không hiện khi chia sẻ màn hình. Xem [hướng dẫn sử dụng](docs/USER-GUIDE.md).

## How it works

```
system audio (the other side) ─┐
microphone (you)               ─┴─► speech recognition ─► question detection ─► search your documents ─► LLM ─► overlay
```

| | Default | Option |
|---|---|---|
| Speech recognition | Whisper `small` on your computer | PhoWhisper for Vietnamese (not wired in yet) |
| Finding the right passage | Words (BM25) in a local SQLite file | Plus meaning, with multilingual-e5-small downloaded once |
| Writing the answer | Qwen3 4B or 1.7B on your computer, picked by your RAM | A provider over the network: OpenAI, OpenCode Zen or Go, or any OpenAI-compatible endpoint |

Models are downloaded on first use and checked against a SHA-256, never bundled.

## Quick start

Prerequisites: Rust stable, Node 22, `cmake` and a C++ toolchain, and the [Tauri v2 system prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS.

```bash
npm run setup     # install frontend deps
npm run dev       # build the local model sidecar, then run the desktop app
npm run check     # fmt + clippy + tests + typecheck (same as CI)
npm run build     # installers in target/release/bundle
```

`npm run ui` runs only the interface in a browser, with sample data, for styling work.

## Using it

See the [user guide](docs/USER-GUIDE.md) ([English](docs/USER-GUIDE.en.md)). The first launch walks you through the language, the speech model, audio permissions and where the AI runs.

### Teleprompter keys

| Key | Action |
|---|---|
| Space | Play / pause |
| ↑ / ↓ | Speed |
| + / − | Font size |
| M | Mirror |
| E | Edit script (Esc to finish) |
| Home | Back to top |
| T, 1, 2 | Captions, microphone, system audio |
| Esc | Stop |

Global shortcuts (work when Offstage is not in front): `Ctrl/⌘+Alt+Space` play/pause, `Ctrl/⌘+Alt+↑/↓` speed, `Ctrl/⌘+Alt+O` hide/show, `Ctrl/⌘+Alt+T` click-through.

## Screen-share exclusion

| OS | Mechanism | Notes |
|---|---|---|
| Windows 10 2004+ / 11 | `WDA_EXCLUDEFROMCAPTURE` | The window is removed from capture |
| Older Windows 10 | `WDA_MONITOR` | Viewers see a black box |
| macOS | `NSWindow.sharingType = .none` | Apps using ScreenCaptureKit on macOS 15+ may still capture it |

Physical cameras and capture cards can always see the screen. Results per app: [docs/COMPATIBILITY.md](docs/COMPATIBILITY.md).

## Privacy

- Local by default. Audio is never written to disk and never leaves your computer.
- Network use is opt-in. With a provider over the network, the question and a few lines of speech around it are sent as text, together with the passages of your documents the question matched. The overlay says so. A workspace, or the whole app, can be set to **local only**.
- API keys live in the OS keychain, never in config files or the repo, and every call is made from the Rust side, never from the web view.
- Documents stay in your own local database, one file per user.

## Responsible use

Offstage is a general-purpose tool. You are responsible for following the rules of the meetings, interviews, exams and organisations you use it in, and for any recording-consent and personal-data laws that apply to you. Many employers and exam bodies prohibit undisclosed AI assistance.

## Project

[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) · [docs/ROADMAP.md](docs/ROADMAP.md) · [docs/RELEASE.md](docs/RELEASE.md) · [CLAUDE.md](CLAUDE.md) (conventions and decisions)

## License

MIT. Models are downloaded separately and keep their own licenses (Whisper: MIT, multilingual-e5-small: MIT, Qwen3: Apache-2.0).
