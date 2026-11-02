import { useCallback, useEffect, useState } from "react";
import {
  activeWorkspace,
  createWorkspace,
  deleteWorkspace,
  errorMessage,
  listWorkspaces,
  onWorkspaceChanged,
  setActiveWorkspace,
  updateWorkspace,
  type Workspace,
  type WorkspaceUpdate,
} from "./tauri";

export type Workspaces = ReturnType<typeof useWorkspaces>;

/**
 * The workspaces and the one in use. Storage and the rules (the last one cannot be deleted, a
 * model belongs to its provider) live in Rust; this keeps what is on screen and asks Rust to
 * change it. Every change comes back through the `workspace_changed` event, so two screens
 * showing workspaces never disagree.
 */
export function useWorkspaces() {
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [active, setActive] = useState<Workspace | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const [all, current] = await Promise.all([listWorkspaces(), activeWorkspace()]);
      setWorkspaces(all);
      setActive(current);
    } catch (e) {
      setError(errorMessage(e));
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    let off: (() => void) | undefined;
    void refresh();
    void onWorkspaceChanged(() => void refresh()).then((unlisten) => {
      if (cancelled) unlisten();
      else off = unlisten;
    });
    return () => {
      cancelled = true;
      off?.();
    };
  }, [refresh]);

  /** Runs a change and shows its error. Resolves to `false` when it failed. */
  const run = useCallback(
    async (op: () => Promise<unknown>): Promise<boolean> => {
      try {
        setError(null);
        await op();
        await refresh();
        return true;
      } catch (e) {
        setError(errorMessage(e));
        return false;
      }
    },
    [refresh],
  );

  return {
    workspaces,
    active,
    error,
    clearError: useCallback(() => setError(null), []),
    select: useCallback((id: string) => run(() => setActiveWorkspace(id)), [run]),
    create: useCallback((name: string) => run(() => createWorkspace(name)), [run]),
    update: useCallback((id: string, update: WorkspaceUpdate) => run(() => updateWorkspace(id, update)), [run]),
    remove: useCallback((id: string) => run(() => deleteWorkspace(id)), [run]),
  };
}
