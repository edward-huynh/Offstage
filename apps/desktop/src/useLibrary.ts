import { useCallback, useEffect, useState } from "react";
import { loadConfig, saveConfig } from "./tauri";
import { useScripts } from "./useScripts";

/**
 * The library of contents, shared by the sidebar list and the Content screen. The content that
 * is open is remembered in the settings, so Home preselects the same one.
 */
export function useLibrary() {
  const [activeId, setActiveId] = useState<string | null>(null);
  const [ready, setReady] = useState(false);

  useEffect(() => {
    void loadConfig().then((config) => {
      setActiveId(config.active_script);
      setReady(true);
    });
  }, []);

  const remember = useCallback(async (id: string) => {
    try {
      const config = await loadConfig();
      if (config.active_script !== id) await saveConfig({ ...config, active_script: id });
    } catch {
      // Not remembering it is harmless.
    }
  }, []);

  return { ...useScripts(ready, activeId, remember), ready };
}

export type Library = ReturnType<typeof useLibrary>;
