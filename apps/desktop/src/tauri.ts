// Thin wrapper so the UI also runs in a normal browser (`npm run ui`) for quick styling work.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { translateError } from "./i18n/errors";

export type OverlayConfig = {
  font_size: number;
  opacity: number;
  scroll_speed: number;
  mirror: boolean;
  reading_line: number;
  click_through: boolean;
  active_script: string | null;
};

const DEFAULTS: OverlayConfig = {
  font_size: 32,
  opacity: 0.85,
  scroll_speed: 40,
  mirror: false,
  reading_line: 0.38,
  click_through: false,
  active_script: null,
};

/** How well the OS can keep the overlay out of screen shares. Decided in Rust. */
export type CaptureProtection = "full" | "black_box" | "best_effort" | "unknown";

export async function captureProtection(): Promise<CaptureProtection> {
  if (!inTauri()) return "unknown";
  try {
    return await invoke<CaptureProtection>("capture_protection_status");
  } catch {
    return "unknown";
  }
}

export type ScriptMeta = { id: string; title: string; updated_at: number };

// Without Tauri (`npm run ui`) there is no storage, so one fixed script stands in for styling work.
const BROWSER_SCRIPT: ScriptMeta = { id: "browser", title: "Kịch bản mẫu", updated_at: 0 };
const BROWSER_BODY = "Đây là bản chạy trong trình duyệt.\n\nChạy `npm run dev` để dùng nhiều kịch bản.";

export const inTauri = () => "__TAURI_INTERNALS__" in window;

export type WindowView = "main" | "overlay";

/**
 * Which window this page is in. Both windows load the same page, so this decides what to draw.
 * In a browser (`npm run ui`) there is no window, so `?view=overlay` picks the overlay.
 */
export function windowView(): WindowView {
  if (inTauri()) return getCurrentWindow().label === "overlay" ? "overlay" : "main";
  return new URLSearchParams(window.location.search).get("view") === "overlay" ? "overlay" : "main";
}

export async function loadConfig(): Promise<OverlayConfig> {
  if (!inTauri()) return DEFAULTS;
  try {
    return await invoke<OverlayConfig>("get_config");
  } catch {
    return DEFAULTS;
  }
}

/** Sends the config to Rust, which clamps and persists it. */
export async function saveConfig(config: OverlayConfig): Promise<void> {
  if (!inTauri()) return;
  try {
    await invoke("update_config", { config });
  } catch {
    // Persisting is best effort: the overlay keeps working with the in-memory value.
  }
}

// The script commands let errors propagate so the panel can show them. Rust sends the
// message as a plain string, so a rejection is usually a string, not an Error.
export function errorMessage(e: unknown): string {
  if (typeof e === "string") return translateError(e);
  if (e instanceof Error) return translateError(e.message);
  return translateError("Có lỗi không xác định");
}

export async function listScripts(): Promise<ScriptMeta[]> {
  if (!inTauri()) return [BROWSER_SCRIPT];
  return invoke<ScriptMeta[]>("list_scripts");
}

export async function createScript(title: string, body: string): Promise<ScriptMeta> {
  if (!inTauri()) return BROWSER_SCRIPT;
  return invoke<ScriptMeta>("create_script", { title, body });
}

export async function readScript(id: string): Promise<string> {
  if (!inTauri()) return BROWSER_BODY;
  return invoke<string>("read_script", { id });
}

export async function saveScript(id: string, body: string): Promise<ScriptMeta> {
  if (!inTauri()) return BROWSER_SCRIPT;
  return invoke<ScriptMeta>("save_script", { id, body });
}

export async function renameScript(id: string, title: string): Promise<ScriptMeta> {
  if (!inTauri()) return { ...BROWSER_SCRIPT, title };
  return invoke<ScriptMeta>("rename_script", { id, title });
}

/** A search result: the script, and the text around the first match when the text matched. */
export type ScriptHit = { id: string; title: string; snippet: string | null };

/** Searches titles and text, ignoring case and Vietnamese accents. Newest first, titles before text. */
export async function searchScripts(query: string): Promise<ScriptHit[]> {
  if (!inTauri()) {
    const q = query.trim().toLowerCase();
    if (!q) return [];
    const inTitle = BROWSER_SCRIPT.title.toLowerCase().includes(q);
    const inBody = BROWSER_BODY.toLowerCase().includes(q);
    return inTitle || inBody
      ? [{ id: BROWSER_SCRIPT.id, title: BROWSER_SCRIPT.title, snippet: inBody ? BROWSER_BODY : null }]
      : [];
  }
  return invoke<ScriptHit[]>("search_scripts", { query });
}

export async function deleteScript(id: string): Promise<void> {
  if (!inTauri()) return;
  await invoke("delete_script", { id });
}

/** Opens a file picker in Rust. Resolves to `null` when the user cancels. */
export async function importScript(): Promise<ScriptMeta | null> {
  if (!inTauri()) return null;
  return invoke<ScriptMeta | null>("import_script");
}

/** Hotkey actions the UI handles itself. Show/hide is done entirely in Rust. */
export type HotkeyAction = "play_pause" | "speed_up" | "speed_down";

/** Subscribes to global hotkeys. Resolves to the function that unsubscribes. */
export async function onHotkey(handler: (action: HotkeyAction) => void): Promise<() => void> {
  if (!inTauri()) return () => {};
  return listen<HotkeyAction>("hotkey", (event) => handler(event.payload));
}

/** Asks Rust to turn click-through on or off. Rust applies it and replies with an event. */
export async function setClickThrough(enabled: boolean): Promise<void> {
  if (!inTauri()) return;
  try {
    await invoke("set_click_through", { enabled });
  } catch {
    // The indicator only changes on the event, so a failure leaves it showing the truth.
  }
}

/** Subscribes to click-through changes, whether made by hotkey, tray or this window. */
export async function onClickThrough(handler: (enabled: boolean) => void): Promise<() => void> {
  if (!inTauri()) return () => {};
  return listen<boolean>("click_through", (event) => handler(event.payload));
}

/** Shortcuts Rust could not register (for example, taken by another app). */
export async function hotkeyFailures(): Promise<string[]> {
  if (!inTauri()) return [];
  try {
    return await invoke<string[]>("hotkey_failures");
  } catch {
    return [];
  }
}

/** Which side of the call a transcript line came from: the microphone or the system audio. */
export type Speaker = "me" | "remote";

export type LaneStatus = {
  speaker: Speaker;
  utterances: number;
  dropped: number;
  /** From a finished utterance to its transcript, leaving out the closing silence. */
  last_latency_ms: number | null;
};

export type ListeningStatus = { lanes: LaneStatus[]; busy: boolean };

export type ModelInfo = {
  spec: {
    id: string;
    file_name: string;
    size: number;
    license: string;
    license_url: string | null;
  };
  installed: boolean;
};

/** What `start_listening` fails with when the speech model has not been downloaded. */
export const MODEL_MISSING = "model_missing";

/** What the system lets the app do with an audio source. */
export type Access = "granted" | "denied" | "not_determined" | "unknown";

export type AudioPermissions = { microphone: Access; system_audio: Access };

/**
 * The audio permissions as far as they can be read. The microphone one can be; the system audio one
 * shows as `unknown` until a start attempt fails (macOS has no public call for it), and Windows
 * needs neither.
 */
export async function audioPermissions(): Promise<AudioPermissions> {
  if (!inTauri()) return { microphone: "unknown", system_audio: "unknown" };
  return invoke<AudioPermissions>("audio_permissions");
}

/** Asks for the microphone if the user has not decided, and resolves with their answer. */
export async function requestMicrophoneAccess(): Promise<Access> {
  if (!inTauri()) return "unknown";
  return invoke<Access>("request_microphone_access");
}

/** A privacy page in the system settings. */
export type PrivacyPane = "microphone" | "system_audio";

/**
 * `start_listening` fails with `permission_denied:<source>` when the OS will not let the app
 * hear that source. Returns the source, or `null` for any other error.
 */
export function deniedPermission(message: string): PrivacyPane | null {
  const [code, source] = message.split(":");
  if (code !== "permission_denied") return null;
  return source === "microphone" || source === "system_audio" ? source : null;
}

export async function openPrivacySettings(pane: PrivacyPane): Promise<void> {
  if (!inTauri()) return;
  await invoke("open_privacy_settings", { pane });
}

// These calls let errors propagate so the panel can show them (see `errorMessage`).
export async function startListening(speaker: Speaker): Promise<void> {
  if (!inTauri()) return;
  await invoke("start_listening", { speaker });
}

export async function stopListening(speaker: Speaker): Promise<void> {
  if (!inTauri()) return;
  await invoke("stop_listening", { speaker });
}

export async function pipelineStatus(): Promise<ListeningStatus> {
  if (!inTauri()) return { lanes: [], busy: false };
  try {
    return await invoke<ListeningStatus>("pipeline_status");
  } catch {
    return { lanes: [], busy: false };
  }
}

export async function modelInfo(): Promise<ModelInfo | null> {
  // Without Tauri there is no model, so a sample stands in for styling work.
  if (!inTauri()) {
    return {
      spec: { id: "demo", file_name: "demo.bin", size: 190_085_487, license: "MIT", license_url: null },
      installed: true,
    };
  }
  try {
    return await invoke<ModelInfo>("model_info");
  } catch {
    return null;
  }
}

/** Resolves when the model is complete and verified. Progress arrives through `onModelProgress`. */
export async function downloadModel(): Promise<void> {
  if (!inTauri()) return;
  await invoke("download_model");
}

export async function cancelDownload(): Promise<void> {
  if (!inTauri()) return;
  await invoke("cancel_download");
}

type TranscriptEvent = { type: "transcript"; speaker: Speaker; text: string; is_final: boolean };
type ErrorEvent = { type: "error"; message: string };
type ModelProgressEvent = { type: "model_progress"; id: string; downloaded: number; total: number };

export async function onTranscript(
  handler: (speaker: Speaker, text: string) => void,
): Promise<() => void> {
  if (!inTauri()) return () => {};
  return listen<TranscriptEvent>("transcript", (event) =>
    handler(event.payload.speaker, event.payload.text),
  );
}

/** Problems the pipeline reports while running, such as transcription falling behind. */
export async function onListeningError(handler: (message: string) => void): Promise<() => void> {
  if (!inTauri()) return () => {};
  return listen<ErrorEvent>("error", (event) => handler(event.payload.message));
}

/** The id the embedding model's `model_progress` events carry. */
export const EMBEDDING_MODEL_ID = "multilingual-e5-small";

/** Every download reports through `model_progress`; these tell whose event it is. */
export const isSpeechModelId = (id: string) => id.startsWith("whisper");
export const isLocalLlmId = (id: string) => id.startsWith("qwen3");

/** `accept` picks which model's events to follow: every download uses the same event. */
export async function onModelProgress(
  handler: (downloaded: number, total: number) => void,
  accept: (modelId: string) => boolean = () => true,
): Promise<() => void> {
  if (!inTauri()) return () => {};
  return listen<ModelProgressEvent>("model_progress", (event) => {
    if (accept(event.payload.id)) handler(event.payload.downloaded, event.payload.total);
  });
}

export type PlayMode = "content" | "interview";

/** What the overlay is playing: one content, or an interview with live captions. */
export type PlaySession = { mode: PlayMode; content_id: string | null };

/** Opens the overlay for `session` and hides the main window. Errors propagate. */
export async function startPlay(session: PlaySession): Promise<void> {
  if (!inTauri()) return;
  await invoke("start_play", { session });
}

/** Stops listening, closes the overlay and brings the main window back. */
export async function stopPlay(): Promise<void> {
  if (!inTauri()) return;
  await invoke("stop_play");
}

export async function getPlaySession(): Promise<PlaySession | null> {
  if (!inTauri()) {
    // Outside Tauri the overlay page stands alone, so it plays the fixed sample.
    const mode = new URLSearchParams(window.location.search).get("mode");
    return mode === "interview"
      ? { mode: "interview", content_id: null }
      : { mode: "content", content_id: BROWSER_SCRIPT.id };
  }
  try {
    return await invoke<PlaySession | null>("get_play_session");
  } catch {
    return null;
  }
}

export async function onPlayStarted(handler: (session: PlaySession) => void): Promise<() => void> {
  if (!inTauri()) return () => {};
  return listen<PlaySession>("play_started", (event) => handler(event.payload));
}

export async function onPlayStopped(handler: () => void): Promise<() => void> {
  if (!inTauri()) return () => {};
  return listen("play_stopped", () => handler());
}

/** A set of documents with a note for the model and the choices that go with one situation. */
export type Workspace = {
  id: string;
  name: string;
  /** What the user wants the model to know about this situation. */
  note: string;
  /** Nothing said in a call using this workspace may leave the machine. */
  local_only: boolean;
  /** Answer with this provider instead of the app-wide one, or `null`. */
  provider_id: string | null;
  /** The model of that provider, or `null`. */
  model: string | null;
  /** Unix time in milliseconds. */
  created_at: number;
};

/** Everything that can be changed about a workspace. Each field replaces the stored one. */
export type WorkspaceUpdate = Pick<Workspace, "name" | "note" | "local_only" | "provider_id" | "model">;

// Without Tauri there is no storage, so workspaces live in memory for styling work.
let browserWorkspaces: Workspace[] = [
  { id: "default", name: "Mặc định", note: "", local_only: false, provider_id: null, model: null, created_at: 0 },
];
let browserActiveWorkspace = "default";
const workspaceListeners = new Set<() => void>();
const notifyWorkspaces = () => workspaceListeners.forEach((fn) => fn());

/** Every workspace, oldest first. */
export async function listWorkspaces(): Promise<Workspace[]> {
  if (!inTauri()) return browserWorkspaces;
  return invoke<Workspace[]>("list_workspaces");
}

/** The workspace in use. */
export async function activeWorkspace(): Promise<Workspace> {
  if (!inTauri()) return browserWorkspaces.find((w) => w.id === browserActiveWorkspace) ?? browserWorkspaces[0];
  return invoke<Workspace>("active_workspace");
}

export async function setActiveWorkspace(id: string): Promise<Workspace> {
  if (!inTauri()) {
    browserActiveWorkspace = id;
    notifyWorkspaces();
    return activeWorkspace();
  }
  return invoke<Workspace>("set_active_workspace", { id });
}

export async function createWorkspace(name: string): Promise<Workspace> {
  if (!inTauri()) {
    if (name.trim() === "") throw new Error("Tên workspace không được để trống");
    const workspace: Workspace = {
      id: `w${Date.now()}`,
      name: name.trim(),
      note: "",
      local_only: false,
      provider_id: null,
      model: null,
      created_at: Date.now(),
    };
    browserWorkspaces = [...browserWorkspaces, workspace];
    notifyWorkspaces();
    return workspace;
  }
  return invoke<Workspace>("create_workspace", { name });
}

export async function updateWorkspace(id: string, update: WorkspaceUpdate): Promise<Workspace> {
  if (!inTauri()) {
    if (update.name.trim() === "") throw new Error("Tên workspace không được để trống");
    browserWorkspaces = browserWorkspaces.map((w) => (w.id === id ? { ...w, ...update } : w));
    notifyWorkspaces();
    return browserWorkspaces.find((w) => w.id === id) ?? browserWorkspaces[0];
  }
  return invoke<Workspace>("update_workspace", { id, update });
}

/** Deletes the workspace and its documents. Rejects when it is the last one. */
export async function deleteWorkspace(id: string): Promise<void> {
  if (!inTauri()) {
    if (browserWorkspaces.length <= 1) throw new Error("Phải còn ít nhất một workspace");
    browserWorkspaces = browserWorkspaces.filter((w) => w.id !== id);
    if (browserActiveWorkspace === id) browserActiveWorkspace = browserWorkspaces[0].id;
    notifyWorkspaces();
    return;
  }
  await invoke("delete_workspace", { id });
}

/** Called after a workspace was created, changed, deleted or put in use. */
export async function onWorkspaceChanged(handler: () => void): Promise<() => void> {
  if (!inTauri()) {
    workspaceListeners.add(handler);
    return () => void workspaceListeners.delete(handler);
  }
  return listen("workspace_changed", () => handler());
}

export type KnowledgeKind = "pdf" | "docx" | "markdown" | "text";

export type KnowledgeDoc = {
  id: string;
  /** File name as shown to the user, without folders. */
  name: string;
  kind: KnowledgeKind;
  size_bytes: number;
  /** Unix time in milliseconds. */
  added_at: number;
};

export type SkippedFile = { name: string; reason: string };

/** Outcome of importing a batch of files: one bad file does not spoil the rest. */
export type ImportReport = { imported: KnowledgeDoc[]; skipped: SkippedFile[] };

// Without Tauri there is no storage, so a few documents stand in for styling work.
// They live in memory, so deleting one in the browser has a visible effect.
let browserDocs: KnowledgeDoc[] = [
  { id: "k3", name: "Mô tả công việc.pdf", kind: "pdf", size_bytes: 248_000, added_at: 3 },
  { id: "k2", name: "CV Nguyễn Văn A.docx", kind: "docx", size_bytes: 61_400, added_at: 2 },
  { id: "k1", name: "Tiêu chí đánh giá.md", kind: "markdown", size_bytes: 2_100, added_at: 1 },
];

/** Documents in the knowledge store, newest first. */
export async function listKnowledge(): Promise<KnowledgeDoc[]> {
  if (!inTauri()) return browserDocs;
  return invoke<KnowledgeDoc[]>("list_knowledge");
}

/**
 * Opens a multi-file picker in Rust, which reads and stores the files. An empty report
 * means the user cancelled.
 */
export async function importKnowledge(): Promise<ImportReport> {
  if (!inTauri()) return { imported: [], skipped: [] };
  return invoke<ImportReport>("import_knowledge");
}

/** Imports files by path, as dropped on the window. Rust checks and reads them. */
export async function importKnowledgePaths(paths: string[]): Promise<ImportReport> {
  if (!inTauri()) return { imported: [], skipped: [] };
  return invoke<ImportReport>("import_knowledge_paths", { paths });
}

/** What the user is doing with files dragged over the window. */
export type FileDrag = { state: "over" } | { state: "leave" } | { state: "drop"; paths: string[] };

/**
 * Files dragged over the window and dropped on it. The system reports the paths, and nothing is
 * read in the web view. Without Tauri there is no such event.
 */
export async function onFileDrag(handler: (drag: FileDrag) => void): Promise<() => void> {
  if (!inTauri()) return () => {};
  return getCurrentWebview().onDragDropEvent((event) => {
    const drag = event.payload;
    if (drag.type === "drop") handler({ state: "drop", paths: drag.paths });
    else if (drag.type === "leave") handler({ state: "leave" });
    else handler({ state: "over" });
  });
}

/**
 * Stores text typed or pasted by the user as a `.md` or `.txt` document. `title` has no
 * extension: Rust adds it. Rejects with a readable message when the text is blank.
 */
export async function createKnowledge(
  title: string,
  kind: Extract<KnowledgeKind, "markdown" | "text">,
  text: string,
): Promise<KnowledgeDoc> {
  if (!inTauri()) {
    if (text.trim() === "") throw new Error("Chưa có nội dung");
    const name = `${title.trim() || "Tài liệu"}.${kind === "markdown" ? "md" : "txt"}`;
    const doc: KnowledgeDoc = {
      id: `k${Date.now()}`,
      name,
      kind,
      size_bytes: new TextEncoder().encode(text).length,
      added_at: Date.now(),
    };
    browserDocs = [doc, ...browserDocs];
    return doc;
  }
  return invoke<KnowledgeDoc>("create_knowledge", { title, kind, text });
}

/** How one document went into the AI's index. A document without an entry is not read yet. */
export type IndexStatus = {
  doc_id: string;
  /** Pieces of text stored. Zero when the document could not be read. */
  chunks: number;
  /** Why it could not be read, in words for the user. */
  error: string | null;
};

/** The state of every document that has been looked at. */
export async function knowledgeIndexStatus(): Promise<IndexStatus[]> {
  if (!inTauri()) {
    return browserDocs.map((d, i) => ({
      doc_id: d.id,
      chunks: i === 1 ? 0 : 6 + i,
      error: i === 1 ? "Tệp không có chữ để đọc. Nếu là PDF quét từ ảnh thì cần một bản có chữ." : null,
    }));
  }
  return invoke<IndexStatus[]>("knowledge_index_status");
}

/** Called after a document was read into the index, or could not be. */
export async function onKnowledgeIndexed(handler: () => void): Promise<() => void> {
  if (!inTauri()) return () => {};
  return listen("knowledge_indexed", () => handler());
}

/** The model that lets the AI find paragraphs by meaning instead of by exact words. */
export type EmbeddingInfo = {
  installed: boolean;
  /** Bytes to download in all. */
  size: number;
  license: string;
  license_url: string | null;
  /** Paragraphs still waiting to be made searchable by meaning. */
  pending_chunks: number;
};

export async function embeddingModelInfo(): Promise<EmbeddingInfo | null> {
  if (!inTauri()) {
    return { installed: false, size: 135_143_000, license: "MIT", license_url: null, pending_chunks: 0 };
  }
  try {
    return await invoke<EmbeddingInfo>("embedding_model_info");
  } catch {
    return null;
  }
}

/** Resolves when every file is complete and verified. Progress arrives through `onModelProgress`. */
export async function downloadEmbeddingModel(): Promise<void> {
  if (!inTauri()) return;
  await invoke("download_embedding_model");
}

export async function cancelEmbeddingDownload(): Promise<void> {
  if (!inTauri()) return;
  await invoke("cancel_embedding_download");
}

/** A language model that can run on this machine. */
export type LocalModelInfo = {
  id: string;
  label: string;
  size: number;
  license: string;
  installed: boolean;
};

export type LocalLlmInfo = {
  models: LocalModelInfo[];
  /** The memory of this computer in bytes, 0 if it could not be read. */
  ram_bytes: number;
  /** The id of the model that "automatic" stands for on this computer. */
  recommended: string;
  /** The program that runs the model is next to the app. False in a development build that did not build it. */
  engine_found: boolean;
};

export async function localLlmInfo(): Promise<LocalLlmInfo | null> {
  if (!inTauri()) {
    return {
      models: [
        { id: "qwen3-4b-q4_k_m", label: "Qwen3 4B", size: 2_497_280_256, license: "Apache-2.0", installed: false },
        { id: "qwen3-1.7b-q4_k_m", label: "Qwen3 1.7B", size: 1_107_409_472, license: "Apache-2.0", installed: false },
      ],
      ram_bytes: 16 * 1024 ** 3,
      recommended: "qwen3-4b-q4_k_m",
      engine_found: true,
    };
  }
  try {
    return await invoke<LocalLlmInfo>("local_llm_info");
  } catch {
    return null;
  }
}

/** Resolves when the file is complete and verified. Progress arrives through `onModelProgress`. */
export async function downloadLocalLlm(id: string): Promise<void> {
  if (!inTauri()) return;
  await invoke("download_local_llm", { id });
}

export async function cancelLocalLlmDownload(): Promise<void> {
  if (!inTauri()) return;
  await invoke("cancel_local_llm_download");
}

export async function deleteKnowledge(id: string): Promise<void> {
  if (!inTauri()) {
    browserDocs = browserDocs.filter((d) => d.id !== id);
    return;
  }
  await invoke("delete_knowledge", { id });
}

export type SpeechLanguage = "auto" | "vietnamese" | "english";

/** Settings that are not about the teleprompter look. Rust clamps and stores them. */
export type AppSettings = {
  speech_language: SpeechLanguage;
  /** `auto` picks by the computer's memory. */
  speech_model: "auto" | "small" | "base";
  interview_system_audio: boolean;
  interview_microphone: boolean;
  hide_main_from_capture: boolean;
  /** The language of the app's own text. */
  ui_language: "vi" | "en";
  /** The first-run walkthrough was finished or skipped. */
  onboarding_done: boolean;
};

const SETTINGS_DEFAULTS: AppSettings = {
  speech_language: "auto",
  speech_model: "auto",
  interview_system_audio: true,
  interview_microphone: false,
  hide_main_from_capture: true,
  ui_language: "vi",
  onboarding_done: false,
};

const settingsListeners = new Set<(settings: AppSettings) => void>();

let browserSettings: AppSettings = SETTINGS_DEFAULTS;

export async function getSettings(): Promise<AppSettings> {
  if (!inTauri()) return browserSettings;
  return invoke<AppSettings>("get_settings");
}

/** Resolves to what was stored, which can differ from the request when a value is corrected. */
export async function updateSettings(settings: AppSettings): Promise<AppSettings> {
  if (!inTauri()) {
    // Mirrors the rule in Rust: an interview must listen to something.
    const fixed =
      settings.interview_system_audio || settings.interview_microphone
        ? settings
        : { ...settings, interview_system_audio: true };
    browserSettings = fixed;
    settingsListeners.forEach((fn) => fn(fixed));
    return fixed;
  }
  return invoke<AppSettings>("update_settings", { settings });
}

/** Called with the settings after any window saved them, so every window can follow a change. */
export async function onSettingsChanged(handler: (settings: AppSettings) => void): Promise<() => void> {
  if (!inTauri()) {
    settingsListeners.add(handler);
    return () => void settingsListeners.delete(handler);
  }
  return listen<AppSettings>("settings_changed", (event) => handler(event.payload));
}

/** Every action that has a global shortcut, including those Rust handles itself. */
export type ShortcutAction = HotkeyAction | "toggle_visibility" | "toggle_click_through";

/** A global shortcut in use. `is_default` is false when the user changed it. */
export type HotkeyBinding = { action: ShortcutAction; accelerator: string; is_default: boolean };

const DEFAULT_BINDINGS: HotkeyBinding[] = [
  { action: "play_pause", accelerator: "CommandOrControl+Alt+Space", is_default: true },
  { action: "speed_up", accelerator: "CommandOrControl+Alt+Up", is_default: true },
  { action: "speed_down", accelerator: "CommandOrControl+Alt+Down", is_default: true },
  { action: "toggle_visibility", accelerator: "CommandOrControl+Alt+O", is_default: true },
  { action: "toggle_click_through", accelerator: "CommandOrControl+Alt+T", is_default: true },
];

// Without Tauri nothing is registered, so changes are only kept in memory for styling work.
let browserBindings: HotkeyBinding[] = DEFAULT_BINDINGS;

/** Every global shortcut in use: the user's own, or the default. */
export async function hotkeyBindings(): Promise<HotkeyBinding[]> {
  if (!inTauri()) return browserBindings;
  return invoke<HotkeyBinding[]>("hotkey_bindings");
}

/**
 * Gives an action another shortcut. Rejects with the reason when Rust refuses it (no Ctrl, Alt or
 * ⌘, another action uses it, another app owns it). Resolves to the bindings after the change.
 */
export async function setHotkey(action: ShortcutAction, accelerator: string): Promise<HotkeyBinding[]> {
  if (!inTauri()) {
    browserBindings = browserBindings.map((b) =>
      b.action === action ? { ...b, accelerator, is_default: false } : b,
    );
    return browserBindings;
  }
  return invoke<HotkeyBinding[]>("set_hotkey", { action, accelerator });
}

/** Puts an action back on its default shortcut. */
export async function resetHotkey(action: ShortcutAction): Promise<HotkeyBinding[]> {
  if (!inTauri()) {
    const original = DEFAULT_BINDINGS.find((b) => b.action === action);
    browserBindings = browserBindings.map((b) => (b.action === action && original ? original : b));
    return browserBindings;
  }
  return invoke<HotkeyBinding[]>("reset_hotkey", { action });
}

/** What the user chose for the assistant. The key is never part of it. */
export type AssistantConfig = {
  enabled: boolean;
  provider_id: string;
  model: string | null;
  custom_base_url: string | null;
  /** Answers are written on this machine only: nothing is sent over the network. */
  local_only: boolean;
  /** When the provider fails, the model on this machine answers. */
  fallback_to_local: boolean;
  /** Id of the model on this machine, see `LocalModelInfo`, or `"auto"`: the one that suits this computer's memory. */
  local_model: string;
};

export type AssistantStatus = {
  enabled: boolean;
  provider_id: string;
  model: string | null;
  has_key: boolean;
  needs_key: boolean;
  /** What is said in the call is sent to another machine. */
  cloud: boolean;
  /** Enabled, with something that can answer: a provider with a model and a key, or the model on this machine. */
  ready: boolean;
  local_only: boolean;
  /** The model on this machine chosen in the settings is downloaded. */
  local_installed: boolean;
  fallback: boolean;
};

/** The `local_model` setting that lets the app pick the model by the computer's memory. */
export const AUTO_LOCAL_MODEL = "auto";

const ASSISTANT_DEFAULTS: AssistantConfig = {
  enabled: false,
  provider_id: "openai",
  model: null,
  custom_base_url: null,
  local_only: false,
  fallback_to_local: true,
  local_model: AUTO_LOCAL_MODEL,
};

let browserAssistant: AssistantConfig = ASSISTANT_DEFAULTS;

export async function getAssistantConfig(): Promise<AssistantConfig> {
  if (!inTauri()) return browserAssistant;
  return invoke<AssistantConfig>("assistant_get_config");
}

/** Resolves to what was stored. Turning the assistant on fails when the provider is unusable. */
export async function setAssistantConfig(config: AssistantConfig): Promise<AssistantConfig> {
  if (!inTauri()) {
    browserAssistant = config;
    return config;
  }
  return invoke<AssistantConfig>("assistant_set_config", { config });
}

export async function assistantStatus(): Promise<AssistantStatus> {
  if (!inTauri()) {
    return {
      enabled: browserAssistant.enabled,
      provider_id: browserAssistant.provider_id,
      model: browserAssistant.model,
      has_key: false,
      needs_key: true,
      cloud: !browserAssistant.local_only,
      ready: false,
      local_only: browserAssistant.local_only,
      local_installed: false,
      fallback: browserAssistant.fallback_to_local,
    };
  }
  return invoke<AssistantStatus>("assistant_status");
}

/** Saves the key in the system keychain. It cannot be read back. */
export async function setAssistantKey(providerId: string, key: string): Promise<void> {
  if (!inTauri()) return;
  await invoke("assistant_set_key", { providerId, key });
}

export async function clearAssistantKey(providerId: string): Promise<void> {
  if (!inTauri()) return;
  await invoke("assistant_clear_key", { providerId });
}

/** Checks the saved provider, key and address. Rejects with the reason. */
export async function testAssistantConnection(): Promise<void> {
  if (!inTauri()) throw new Error("Chỉ chạy được trong ứng dụng.");
  await invoke("assistant_test_connection");
}

export async function listAssistantModels(): Promise<string[]> {
  if (!inTauri()) return [];
  return invoke<string[]>("assistant_list_models");
}

type QuestionEvent = { type: "question_detected"; text: string };
type AnswerTokenEvent = { type: "answer_token"; token: string };
type AnswerDoneEvent = { type: "answer_done"; sources: string[]; fell_back?: boolean; local?: boolean };

export async function onQuestionDetected(handler: (text: string) => void): Promise<() => void> {
  if (!inTauri()) return () => {};
  return listen<QuestionEvent>("question_detected", (event) => handler(event.payload.text));
}

export async function onAnswerToken(handler: (token: string) => void): Promise<() => void> {
  if (!inTauri()) return () => {};
  return listen<AnswerTokenEvent>("answer_token", (event) => handler(event.payload.token));
}

/** How an answer ended: where it came from and what it was written from. */
export type AnswerOutcome = {
  /** The documents the answer was written from, best first. */
  sources: string[];
  /** Written by the model on this machine. */
  local: boolean;
  /** The provider failed, so the model on this machine wrote it. */
  fellBack: boolean;
};

export async function onAnswerDone(handler: (outcome: AnswerOutcome) => void): Promise<() => void> {
  if (!inTauri()) return () => {};
  return listen<AnswerDoneEvent>("answer_done", (event) =>
    handler({
      sources: event.payload.sources ?? [],
      local: event.payload.local ?? false,
      fellBack: event.payload.fell_back ?? false,
    }),
  );
}
