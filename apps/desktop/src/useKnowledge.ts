import { useCallback, useEffect, useRef, useState } from "react";
import {
  createKnowledge,
  deleteKnowledge,
  errorMessage,
  importKnowledge,
  importKnowledgePaths,
  knowledgeIndexStatus,
  listKnowledge,
  onKnowledgeIndexed,
  onWorkspaceChanged,
  type ImportReport,
  type IndexStatus,
  type KnowledgeDoc,
} from "./tauri";

/**
 * Holds the list of knowledge documents. Storage and file reading live in Rust; this only
 * keeps what is on screen and asks Rust to import or delete.
 */
export function useKnowledge() {
  const [docs, setDocs] = useState<KnowledgeDoc[]>([]);
  /** By document id. A document missing here is still being read. */
  const [index, setIndex] = useState<Record<string, IndexStatus>>({});
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

  // Another workspace has other documents.
  useEffect(() => {
    let cancelled = false;
    let off: (() => void) | undefined;
    void onWorkspaceChanged(() => void refresh()).then((unlisten) => {
      if (cancelled) unlisten();
      else off = unlisten;
    });
    return () => {
      cancelled = true;
      off?.();
    };
  }, [refresh]);

  // Rust reads each document into the AI's index in the background and says when one is done.
  useEffect(() => {
    let cancelled = false;
    let off: (() => void) | undefined;
    const load = async () => {
      try {
        const statuses = await knowledgeIndexStatus();
        if (!cancelled) setIndex(Object.fromEntries(statuses.map((s) => [s.doc_id, s])));
      } catch {
        // Without the statuses the screen only shows no state, which is not worth an error banner.
      }
    };
    void load();
    void onKnowledgeIndexed(() => void load()).then((unlisten) => {
      if (cancelled) unlisten();
      else off = unlisten;
    });
    return () => {
      cancelled = true;
      off?.();
    };
  }, []);

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

  /** Imports files dropped on the window. Resolves to the report, or `null` when the call failed. */
  const importPaths = useCallback(
    async (paths: string[]): Promise<ImportReport | null> => {
      if (busy.current || paths.length === 0) return null;
      busy.current = true;
      setImporting(true);
      let report: ImportReport | null = null;
      await run(async () => {
        report = await importKnowledgePaths(paths);
        setDocs(await listKnowledge());
      });
      busy.current = false;
      setImporting(false);
      return report;
    },
    [run],
  );

  /** Stores typed or pasted text as a document. Resolves to `true` once it is saved. */
  const create = useCallback(
    (title: string, kind: "markdown" | "text", text: string) =>
      run(async () => {
        await createKnowledge(title, kind, text);
        setDocs(await listKnowledge());
      }),
    [run],
  );

  const remove = useCallback(
    (id: string) =>
      run(async () => {
        await deleteKnowledge(id);
        setDocs(await listKnowledge());
      }),
    [run],
  );

  const clearError = useCallback(() => setError(null), []);

  return { docs, index, error, importing, clearError, refresh, importFiles, importPaths, create, remove };
}
