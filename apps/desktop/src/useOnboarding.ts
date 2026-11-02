import { useCallback, useEffect, useState } from "react";
import { getSettings, updateSettings } from "./tauri";

/**
 * Whether the first-run walkthrough still has to be shown. The answer is kept in the settings, so
 * it is shown once, until it is finished or skipped.
 */
export function useOnboarding() {
  // `null` until the settings are read: nothing is drawn meanwhile, so the main window does not
  // flash before the walkthrough covers it.
  const [pending, setPending] = useState<boolean | null>(null);

  useEffect(() => {
    let cancelled = false;
    getSettings()
      .then((settings) => {
        if (!cancelled) setPending(!settings.onboarding_done);
      })
      // Without settings there is nothing to remember the answer in, so do not show it.
      .catch(() => {
        if (!cancelled) setPending(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const finish = useCallback(async () => {
    setPending(false);
    try {
      // The freshest settings, so a language picked in the walkthrough is kept.
      await updateSettings({ ...(await getSettings()), onboarding_done: true });
    } catch {
      // Not remembering is harmless: the walkthrough shows once more at the next start.
    }
  }, []);

  return { pending, finish };
}
