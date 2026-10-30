import { useCallback, useEffect, useState } from "react";
import { errorMessage, listScripts, loadConfig, saveConfig, type ScriptMeta } from "./tauri";

/**
 * The saved contents (scripts) and which one is picked on the home screen. The pick is
 * remembered in the settings, so the same content is selected next time.
 */
export function useContents() {
  const [items, setItems] = useState<ScriptMeta[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const [list, config] = await Promise.all([listScripts(), loadConfig()]);
        if (cancelled) return;
        setItems(list);
        setSelected(list.find((s) => s.id === config.active_script)?.id ?? list[0]?.id ?? null);
      } catch (e) {
        if (!cancelled) setError(errorMessage(e));
      } finally {
        if (!cancelled) setLoaded(true);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const select = useCallback(async (id: string) => {
    setSelected(id);
    try {
      const config = await loadConfig();
      await saveConfig({ ...config, active_script: id });
    } catch {
      // Not remembering the pick is harmless: it is still selected on screen.
    }
  }, []);

  return { items, selected, loaded, error, select };
}
