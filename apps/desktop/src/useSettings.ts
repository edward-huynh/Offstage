import { useCallback, useEffect, useRef, useState } from "react";
import {
  errorMessage,
  getSettings,
  loadConfig,
  saveConfig,
  updateSettings,
  type AppSettings,
  type OverlayConfig,
} from "./tauri";

/** How long a slider waits after the last move before the value is written. */
const SAVE_DELAY_MS = 300;

/**
 * The app settings and the teleprompter look, as the Settings screen edits them.
 * Rust clamps and stores both. A change shows on screen at once and is written after a
 * short pause, so dragging a slider does not write on every step.
 */
export function useSettings() {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [overlay, setOverlay] = useState<OverlayConfig | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    Promise.all([getSettings(), loadConfig()])
      .then(([s, o]) => {
        if (cancelled) return;
        setSettings(s);
        setOverlay(o);
      })
      .catch((e) => {
        if (!cancelled) setError(errorMessage(e));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  /** Settings are written at once: they are switches and choices, not sliders. */
  const change = useCallback(
    async (patch: Partial<AppSettings>) => {
      if (!settings) return;
      setError(null);
      setSettings({ ...settings, ...patch });
      try {
        // Rust may correct the value (an interview must listen to something).
        setSettings(await updateSettings({ ...settings, ...patch }));
      } catch (e) {
        setSettings(settings);
        setError(errorMessage(e));
      }
    },
    [settings],
  );

  // Look changes are merged into the freshest saved config, so a pick made elsewhere
  // (the selected content) is not overwritten by the copy loaded when the screen opened.
  const pending = useRef<Partial<OverlayConfig>>({});
  const timer = useRef<number | undefined>(undefined);

  const flush = useCallback(async () => {
    window.clearTimeout(timer.current);
    timer.current = undefined;
    const patch = pending.current;
    pending.current = {};
    if (Object.keys(patch).length === 0) return;
    await saveConfig({ ...(await loadConfig()), ...patch });
  }, []);

  const changeOverlay = useCallback(
    (patch: Partial<OverlayConfig>) => {
      setOverlay((current) => current && { ...current, ...patch });
      pending.current = { ...pending.current, ...patch };
      window.clearTimeout(timer.current);
      timer.current = window.setTimeout(() => void flush(), SAVE_DELAY_MS);
    },
    [flush],
  );

  // Leaving the screen writes what is still waiting for the pause.
  useEffect(
    () => () => {
      void flush();
    },
    [flush],
  );

  return { settings, overlay, error, change, changeOverlay };
}
