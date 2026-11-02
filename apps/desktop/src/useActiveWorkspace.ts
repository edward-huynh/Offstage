import { useEffect, useState } from "react";
import { activeWorkspace, onWorkspaceChanged, type Workspace } from "./tauri";

/** The workspace in use, following changes made from any window. `null` until it is known. */
export function useActiveWorkspace(): Workspace | null {
  const [workspace, setWorkspace] = useState<Workspace | null>(null);

  useEffect(() => {
    let cancelled = false;
    let off: (() => void) | undefined;
    const refresh = () =>
      activeWorkspace()
        .then((w) => {
          if (!cancelled) setWorkspace(w);
        })
        // Without it the overlay simply shows no name.
        .catch(() => {});
    void refresh();
    void onWorkspaceChanged(() => void refresh()).then((unlisten) => {
      if (cancelled) unlisten();
      else off = unlisten;
    });
    return () => {
      cancelled = true;
      off?.();
    };
  }, []);

  return workspace;
}
