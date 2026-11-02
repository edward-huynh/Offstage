# Offstage user guide

Offstage is a transparent layer that sits on top of every app and stays out of your screen shares. It reads a script as a teleprompter, shows live captions of a call and suggests answers from your own documents. Tiếng Việt: [USER-GUIDE.md](USER-GUIDE.md).

## First launch

A walkthrough takes you through five steps, and each can be skipped:

1. **Language** of the app (Tiếng Việt or English).
2. **Speech recognition model**, downloaded once (about 190 MB). The audio is processed on your computer.
3. **Audio permissions.** The microphone is your voice, the system audio is the other side's. macOS only asks for the system audio permission when you start listening for the first time.
4. **AI assistant**: run on this computer (private, needs a downloaded model), use a provider over the network (enter an API key), or later.
5. **All set**: try the sample script or enter the app.

Everything set up in the walkthrough can be changed in Settings.

## Reading a script (Content)

**Content** is where you write scripts. Separate paragraphs with a blank line: each becomes a block when reading. Pick a script on **Home** and press **Play**: the overlay appears and scrolls.

| Key | Action |
|---|---|
| Space | Play / pause |
| ↑ / ↓ | Speed |
| + / − | Font size |
| M | Mirror |
| E | Edit the script (Esc to finish) |
| Home | Back to top |
| Esc | Stop |

Global shortcuts work even when Offstage is not in front: `Ctrl/⌘+Alt+Space` play/pause, `Ctrl/⌘+Alt+↑/↓` speed, `Ctrl/⌘+Alt+O` hide/show, `Ctrl/⌘+Alt+T` click-through.

## Listening to a call (Interview)

On **Home**, press **Listen and show captions**. In the overlay, `1` turns the microphone on, `2` the system audio, `T` opens the captions. The default sources are chosen in Settings, under Speech.

## AI assistant

When the other side asks something, the assistant shows a few bullet points on the overlay, with its sources ("From your documents: CV.pdf › Experience").

1. Turn on the **AI assistant** in Settings, under AI assistant.
2. Choose where it runs:
   - **Local only**: download a model (Qwen3 4B, about 2.5 GB, or 1.7B, about 1.1 GB; "Automatic" picks by your RAM). Nothing is sent anywhere. Slower on a weak computer.
   - **A provider over the network**: OpenAI, OpenCode Zen or Go, or any OpenAI-compatible address. Enter the API key; it is kept in the operating system's keychain. The question and a few lines of speech around it are sent as text, with the passages of your documents the question matched.
   - **Fall back to this computer**: when the provider fails, or writes nothing within 5 seconds, the model on this computer writes instead.
3. Add documents under **Knowledge** (below).

Do not take a suggestion as fact: small models can add details that are not in your documents. Read it before you say it.

## Documents (Knowledge)

Add a CV, a job description, a rubric or notes: `.pdf`, `.docx`, `.md` or `.txt` files (up to 20 MB), or type them. Offstage reads the text on your computer and indexes it to find related passages. Each document shows "Read by the AI", or "Could not read" with the reason (for example a PDF scanned from an image has no text).

- **Search by meaning** (optional): download a small model (about 135 MB) so the AI finds related passages even when the question does not use the words of the documents, even in another language. Without it, search still works by words.
- When search finds nothing and the documents are short (a CV and a job description), Offstage sends all of them.

## Workspaces

Each **Workspace** is for one situation (an interview, a sales call): its own documents, a **note for the AI** (the position, the company, who is listening), and its own choices:

- **Local only** for that workspace, even if the general settings use a provider over the network.
- **Own provider**.

The AI only searches the documents of the workspace in use. Switch with the selector at the top of the sidebar. Deleting a workspace deletes its documents too.

## Privacy and screen sharing

- The overlay is always excluded from screen sharing. The main window is hidden by default too (turn it off in Settings, System), because it shows CVs and job descriptions.
- Physical cameras and capture cards can still see the screen.
- Results per app: [COMPATIBILITY.md](COMPATIBILITY.md). Test with a real viewer before an important meeting.

## Troubleshooting

| Symptom | What to do |
|---|---|
| No captions | Check the microphone and system audio permissions (Settings, Speech), and that the speech model is downloaded. |
| "The model for this computer is not downloaded" | Download it in Settings, AI assistant. A build run from source needs `cargo build -p offstage-llm-host`. |
| The assistant does not answer | Settings, AI assistant: read the status line ("no API key", "model not downloaded"). |
| The local model is slow | Choose Qwen3 1.7B. |
| A document says "Could not read" | A PDF scanned from an image needs a version with text. |

## Responsible use

You are responsible for following the rules of the meetings, interviews, exams and organisations you use Offstage in, and for any recording-consent and personal-data laws that apply to you. Many employers and exam bodies prohibit undisclosed AI assistance.
