import { useCallback, useEffect, useRef, useState } from "react";
import {
  deleteKnowledge,
  errorMessage,
  importKnowledge,
  listKnowledge,
  type ImportReport,
  type KnowledgeDoc,
} from "./tauri";

/**
 * Holds the list of knowledge documents. Storage and file reading live in Rust; this only
 * keeps what is on screen and asks Rust to import or delete.
 */
export function useKnowledge() {
  const [docs, setDocs] = useState<KnowledgeDoc[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [importing, setImporting] = useState(false);
  const busy = useRef(false);

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

  const refresh = useCallback(
    () =>
      run(async () => {
        setDocs(await listKnowledge());
      }),
    [run],
  );

  useEffect(() => {
    void refresh();
  }, [refresh]);

  /**
   * Opens the file picker and stores what was picked. Resolves to the report, which lists
   * the files that were skipped and why, or `null` when the call itself failed.
   */
  const importFiles = useCallback(async (): Promise<ImportReport | null> => {
    // The picker blocks in Rust, so a second click would only queue another dialog.
    if (busy.current) return null;
    busy.current = true;
    setImporting(true);
    let report: ImportReport | null = null;
    await run(async () => {
      report = await importKnowledge();
      setDocs(await listKnowledge());
    });
    busy.current = false;
    setImporting(false);
    return report;
  }, [run]);

  const remove = useCallback(
    (id: string) =>
      run(async () => {
        await deleteKnowledge(id);
        setDocs(await listKnowledge());
      }),
    [run],
  );

  const clearError = useCallback(() => setError(null), []);

  return { docs, error, importing, clearError, refresh, importFiles, remove };
}
