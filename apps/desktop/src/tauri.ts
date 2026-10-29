// Thin wrapper so the UI also runs in a normal browser (`npm run ui`) for quick styling work.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

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
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return "Có lỗi không xác định";
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

export async function onModelProgress(
  handler: (downloaded: number, total: number) => void,
): Promise<() => void> {
  if (!inTauri()) return () => {};
  return listen<ModelProgressEvent>("model_progress", (event) =>
    handler(event.payload.downloaded, event.payload.total),
  );
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

export async function deleteKnowledge(id: string): Promise<void> {
  if (!inTauri()) {
    browserDocs = browserDocs.filter((d) => d.id !== id);
    return;
  }
  await invoke("delete_knowledge", { id });
}
