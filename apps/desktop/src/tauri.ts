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

export type ScriptMeta ={ id: string; title: string; updated_at: number };

// Without Tauri (`npm run ui`) there is no storage, so one fixed script stands in for styling work.
const BROWSER_SCRIPT: ScriptMeta = { id: "browser", title: "Kịch bản mẫu", updated_at: 0 };
const BROWSER_BODY = "Đây là bản chạy trong trình duyệt.\n\nChạy `npm run dev` để dùng nhiều kịch bản.";

const inTauri = () => "__TAURI_INTERNALS__" in window;

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

export async function hideOverlay() {
  if (inTauri()) await getCurrentWindow().hide();
}
