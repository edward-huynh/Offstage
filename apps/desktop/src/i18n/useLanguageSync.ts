import { useEffect } from "react";
import { getSettings, onSettingsChanged } from "../tauri";
import { setLanguage } from ".";

/**
 * Keeps the language of the app's text in step with the saved setting, in every window: it reads
 * the setting at start and follows a change made in another window.
 */
export function useLanguageSync(): void {
  useEffect(() => {
    let cancelled = false;
    let off: (() => void) | undefined;
    void getSettings()
      .then((settings) => {
        if (!cancelled) setLanguage(settings.ui_language);
      })
      // Without settings the app stays in Vietnamese, which is the default anyway.
      .catch(() => {});
    void onSettingsChanged((settings) => setLanguage(settings.ui_language)).then((unlisten) => {
      if (cancelled) unlisten();
      else off = unlisten;
    });
    return () => {
      cancelled = true;
      off?.();
    };
  }, []);
}
