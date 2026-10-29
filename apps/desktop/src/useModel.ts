import { useCallback, useEffect, useState } from "react";
import {
  cancelDownload,
  downloadModel,
  errorMessage,
  modelInfo,
  onModelProgress,
  type ModelInfo,
} from "./tauri";

export type ModelProgress = { downloaded: number; total: number };

/** Whether the speech model is on this machine, and downloading it. */
export function useModel() {
  const [info, setInfo] = useState<ModelInfo | null>(null);
  const [progress, setProgress] = useState<ModelProgress | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void modelInfo().then((i) => {
      if (!cancelled) setInfo(i);
    });
    void onModelProgress((downloaded, total) => setProgress({ downloaded, total })).then((off) => {
      if (cancelled) off();
      else unlisten = off;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  const download = useCallback(async () => {
    setError(null);
    setProgress({ downloaded: 0, total: info?.spec.size ?? 0 });
    try {
      await downloadModel();
      setInfo((i) => (i ? { ...i, installed: true } : i));
    } catch (e) {
      const message = errorMessage(e);
      // Cancelling is the user's own choice, not a failure.
      if (!message.includes("cancelled")) setError(message);
    } finally {
      setProgress(null);
    }
  }, [info]);

  const cancel = useCallback(() => void cancelDownload(), []);

  return { info, installed: info?.installed ?? false, progress, error, download, cancel };
}
