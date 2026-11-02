import { useCallback, useEffect, useState } from "react";
import {
  cancelLocalLlmDownload,
  downloadLocalLlm,
  errorMessage,
  isLocalLlmId,
  localLlmInfo,
  onModelProgress,
  type LocalLlmInfo,
} from "./tauri";

export type LocalLlmProgress = { id: string; downloaded: number; total: number };

/** The language models that can run on this machine: which are here, and downloading one. */
export function useLocalLlm() {
  const [info, setInfo] = useState<LocalLlmInfo | null>(null);
  const [progress, setProgress] = useState<LocalLlmProgress | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setInfo(await localLlmInfo());
  }, []);

  useEffect(() => {
    let cancelled = false;
    let off: (() => void) | undefined;
    void refresh();
    void onModelProgress(
      (downloaded, total) => setProgress((p) => (p ? { ...p, downloaded, total } : p)),
      isLocalLlmId,
    ).then((unlisten) => {
      if (cancelled) unlisten();
      else off = unlisten;
    });
    return () => {
      cancelled = true;
      off?.();
    };
  }, [refresh]);

  const download = useCallback(
    async (id: string, size: number) => {
      setError(null);
      setProgress({ id, downloaded: 0, total: size });
      try {
        await downloadLocalLlm(id);
      } catch (e) {
        const message = errorMessage(e);
        // Cancelling is the user's own choice, not a failure.
        if (!message.includes("cancelled")) setError(message);
      } finally {
        setProgress(null);
        await refresh();
      }
    },
    [refresh],
  );

  const cancel = useCallback(() => void cancelLocalLlmDownload(), []);

  return { info, progress, error, download, cancel, refresh };
}
