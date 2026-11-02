import { useCallback, useEffect, useState } from "react";
import {
  EMBEDDING_MODEL_ID,
  cancelEmbeddingDownload,
  downloadEmbeddingModel,
  embeddingModelInfo,
  errorMessage,
  onKnowledgeIndexed,
  onModelProgress,
  type EmbeddingInfo,
} from "./tauri";

export type EmbeddingProgress = { downloaded: number; total: number };

/**
 * The model that lets the AI find paragraphs by meaning: whether it is here, downloading it, and
 * how many paragraphs still wait to be made searchable with it. Rust does the work.
 */
export function useEmbeddingModel() {
  const [info, setInfo] = useState<EmbeddingInfo | null>(null);
  const [progress, setProgress] = useState<EmbeddingProgress | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setInfo(await embeddingModelInfo());
  }, []);

  useEffect(() => {
    let cancelled = false;
    const unlisteners: Array<() => void> = [];
    const keep = (subscription: Promise<() => void>) =>
      subscription.then((off) => {
        if (cancelled) off();
        else unlisteners.push(off);
      });
    void refresh();
    keep(
      onModelProgress(
        (downloaded, total) => setProgress({ downloaded, total }),
        (id) => id === EMBEDDING_MODEL_ID,
      ),
    );
    // Rust says so after every round of paragraphs it made searchable.
    keep(onKnowledgeIndexed(() => void refresh()));
    return () => {
      cancelled = true;
      unlisteners.forEach((off) => off());
    };
  }, [refresh]);

  const download = useCallback(async () => {
    setError(null);
    setProgress({ downloaded: 0, total: info?.size ?? 0 });
    try {
      await downloadEmbeddingModel();
    } catch (e) {
      const message = errorMessage(e);
      // Cancelling is the user's own choice, not a failure.
      if (!message.includes("cancelled")) setError(message);
    } finally {
      setProgress(null);
      await refresh();
    }
  }, [info, refresh]);

  const cancel = useCallback(() => void cancelEmbeddingDownload(), []);

  return { info, progress, error, download, cancel };
}
