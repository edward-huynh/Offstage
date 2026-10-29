import { useCallback, useEffect, useRef, useState } from "react";
import {
  createScript,
  deleteScript,
  errorMessage,
  importScript,
  listScripts,
  readScript,
  renameScript,
  saveScript,
  type ScriptMeta,
} from "./tauri";

const AUTOSAVE_MS = 800;

/**
 * Holds the script list and the text of the active script. Storage lives in Rust; this
 * only keeps what is on screen and asks Rust to save, with a short pause while typing.
 *
 * `activeId` comes from the persisted config. `onActiveChange` reports the script actually opened.
 */
export function useScripts(
  ready: boolean,
  activeId: string | null,
  onActiveChange: (id: string) => void,
) {
  const [scripts, setScripts] = useState<ScriptMeta[]>([]);
  const [body, setBody] = useState("");
  const [error, setError] = useState<string | null>(null);
  /** The script open in the editor, or `null` when there is none. */
  const [current, setCurrent] = useState<string | null>(null);
  const [saveState, setSaveState] = useState<"saved" | "unsaved" | "saving">("saved");

  const currentId = useRef<string | null>(null);
  const bodyRef = useRef("");
  const dirty = useRef(false);
  const timer = useRef<number | undefined>(undefined);
  const started = useRef(false);

  const reloadList = useCallback(async (): Promise<ScriptMeta[]> => {
    const list = await listScripts();
    setScripts(list);
    return list;
  }, []);

  /** Writes pending edits now. Edits stay marked as unsaved if Rust refuses them. */
  const flush = useCallback(async () => {
    window.clearTimeout(timer.current);
    const id = currentId.current;
    if (!dirty.current || !id) return;
    dirty.current = false;
    setSaveState("saving");
    try {
      await saveScript(id, bodyRef.current);
      // An edit made while saving has already marked the text unsaved again.
      setSaveState(dirty.current ? "unsaved" : "saved");
    } catch (e) {
      dirty.current = true;
      setSaveState("unsaved");
      setError(errorMessage(e));
    }
  }, []);

  const open = useCallback(
    async (id: string) => {
      await flush();
      const text = await readScript(id);
      currentId.current = id;
      bodyRef.current = text;
      dirty.current = false;
      setCurrent(id);
      setBody(text);
      setSaveState("saved");
      onActiveChange(id);
    },
    [flush, onActiveChange],
  );

  /** Runs an operation and shows its error. Resolves to `false` when it failed. */
  const run = useCallback(async (op: () => Promise<void>): Promise<boolean> => {
    try {
      setError(null);
      await op();
      return true;
    } catch (e) {
      setError(errorMessage(e));
      return false;
    }
  }, []);

  // Open the remembered script on start, or the newest one. With none saved the editor stays empty.
  useEffect(() => {
    if (!ready || started.current) return;
    started.current = true;
    void run(async () => {
      const list = await reloadList();
      const id = list.find((s) => s.id === activeId)?.id ?? list[0]?.id;
      if (id) await open(id);
    });
  }, [ready, activeId, reloadList, open, run]);

  const edit = useCallback(
    (text: string) => {
      if (!currentId.current) return;
      bodyRef.current = text;
      dirty.current = true;
      setSaveState("unsaved");
      setBody(text);
      window.clearTimeout(timer.current);
      timer.current = window.setTimeout(() => void flush(), AUTOSAVE_MS);
    },
    [flush],
  );

  const select = useCallback((id: string) => run(() => open(id)), [open, run]);

  /** Creates a blank script and opens it. Resolves to its id, or `null` on failure. */
  const create = useCallback(async (): Promise<string | null> => {
    let id: string | null = null;
    await run(async () => {
      await flush();
      const created = await createScript("", "");
      await reloadList();
      await open(created.id);
      id = created.id;
    });
    return id;
  }, [flush, open, reloadList, run]);

  const rename = useCallback(
    (id: string, title: string) =>
      run(async () => {
        await renameScript(id, title);
        await reloadList();
      }),
    [reloadList, run],
  );

  const remove = useCallback(
    (id: string) =>
      run(async () => {
        const wasOpen = id === currentId.current;
        if (wasOpen) {
          // Pending edits belong to the script being deleted.
          window.clearTimeout(timer.current);
          dirty.current = false;
        }
        await deleteScript(id);
        const list = await reloadList();
        if (!wasOpen) return;
        const next = list[0]?.id;
        if (next) {
          await open(next);
        } else {
          // That was the last one: leave the editor empty rather than inventing a script.
          currentId.current = null;
          bodyRef.current = "";
          setCurrent(null);
          setBody("");
          setSaveState("saved");
        }
      }),
    [open, reloadList, run],
  );

  /** Resolves to `true` when a file was imported and opened. */
  const importFile = useCallback(async (): Promise<boolean> => {
    let imported = false;
    await run(async () => {
      await flush();
      const meta = await importScript();
      if (!meta) return;
      await reloadList();
      await open(meta.id);
      imported = true;
    });
    return imported;
  }, [flush, open, reloadList, run]);

  const refresh = useCallback(
    () =>
      run(async () => {
        await reloadList();
      }),
    [reloadList, run],
  );

  const clearError = useCallback(() => setError(null), []);

  return {
    scripts,
    current,
    saveState,
    body,
    error,
    clearError,
    refresh,
    edit,
    flush,
    select,
    create,
    rename,
    remove,
    importFile,
  };
}
