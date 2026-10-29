import { useCallback, useEffect, useRef, useState } from "react";
import {
  cancelDownload,
  deniedPermission,
  downloadModel,
  errorMessage,
  inTauri,
  MODEL_MISSING,
  modelInfo,
  onListeningError,
  onModelProgress,
  onTranscript,
  openPrivacySettings,
  pipelineStatus,
  startListening,
  stopListening,
  type ModelInfo,
  type PrivacyPane,
  type Speaker,
} from "./tauri";

/** Lines kept on screen. Older ones are dropped, the overlay is a live view, not a log. */
const MAX_LINES = 40;
const STATUS_POLL_MS = 2000;

export type Line = { id: number; speaker: Speaker; text: string };
export type DownloadProgress = { downloaded: number; total: number };

// Without Tauri (`npm run ui`) nothing is heard, so a few lines stand in for styling work.
const DEMO_LINES: Line[] = [
  { id: -3, speaker: "remote", text: "Bạn có thể nói qua về kinh nghiệm làm việc với Rust không?" },
  { id: -2, speaker: "me", text: "Vâng, tôi đã dùng Rust hai năm cho các dịch vụ xử lý âm thanh." },
  { id: -1, speaker: "remote", text: "Còn về hiệu năng thì sao?" },
];

/** Rust reports errors in English. Permission problems are common, so they get plain Vietnamese. */
function friendlyError(message: string): string {
  if (message.startsWith("permission denied")) {
    return "Chưa được cấp quyền truy cập âm thanh. Hãy cho phép trong cài đặt hệ thống rồi thử lại.";
  }
  return message;
}

/**
 * What the transcript panel shows: lines, which sources are on, and the model download.
 * Listening itself, and everything it needs, lives in Rust. This only asks for it and renders
 * what comes back. It keeps collecting lines while the panel is closed.
 *
 * `panelOpen` only decides whether to poll for status, which is what the panel displays.
 */
export function useListening(panelOpen: boolean) {
  const [lines, setLines] = useState<Line[]>(() => (inTauri() ? [] : DEMO_LINES));
  const [active, setActive] = useState<Record<Speaker, boolean>>({ me: false, remote: false });
  const [busy, setBusy] = useState<Speaker | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [model, setModel] = useState<ModelInfo | null>(null);
  const [needsModel, setNeedsModel] = useState(false);
  const [progress, setProgress] = useState<DownloadProgress | null>(null);
  const [latencyMs, setLatencyMs] = useState<number | null>(null);
  /** The source the OS refused to let the app hear, until the user retries. */
  const [permissionIssue, setPermissionIssue] = useState<PrivacyPane | null>(null);

  const nextId = useRef(0);
  /** The source the user asked for when the model turned out to be missing. */
  const waitingForModel = useRef<Speaker | null>(null);

  useEffect(() => {
    let cancelled = false;
    const unlisteners: Array<() => void> = [];
    const keep = (subscription: Promise<() => void>) =>
      subscription.then((off) => {
        if (cancelled) off();
        else unlisteners.push(off);
      });

    keep(
      onTranscript((speaker, text) => {
        setLines((previous) => [...previous, { id: nextId.current++, speaker, text }].slice(-MAX_LINES));
      }),
    );
    keep(onListeningError((message) => setError(friendlyError(message))));
    keep(onModelProgress((downloaded, total) => setProgress({ downloaded, total })));
    void modelInfo().then(setModel);

    return () => {
      cancelled = true;
      unlisteners.forEach((off) => off());
    };
  }, []);

  const refreshStatus = useCallback(async () => {
    const { lanes } = await pipelineStatus();
    setActive({
      me: lanes.some((l) => l.speaker === "me"),
      remote: lanes.some((l) => l.speaker === "remote"),
    });
    const latencies = lanes.flatMap((l) => (l.last_latency_ms === null ? [] : [l.last_latency_ms]));
    setLatencyMs(latencies.length > 0 ? Math.max(...latencies) : null);
  }, []);

  // The panel shows the latest speech-to-text latency, so keep it fresh while it is open and listening.
  const listening = active.me || active.remote;
  useEffect(() => {
    if (!panelOpen) return;
    void refreshStatus();
    if (!listening) return;
    const id = window.setInterval(() => void refreshStatus(), STATUS_POLL_MS);
    return () => window.clearInterval(id);
  }, [panelOpen, listening, refreshStatus]);

  /** Turns one source on or off. A missing model leads to the download offer instead of an error. */
  const toggle = useCallback(
    async (speaker: Speaker) => {
      setError(null);
      setPermissionIssue(null);
      setBusy(speaker);
      try {
        if (active[speaker]) await stopListening(speaker);
        else await startListening(speaker);
      } catch (e) {
        const message = errorMessage(e);
        const denied = deniedPermission(message);
        if (message === MODEL_MISSING) {
          waitingForModel.current = speaker;
          setNeedsModel(true);
        } else if (denied) {
          setPermissionIssue(denied);
        } else {
          setError(friendlyError(message));
        }
      } finally {
        setBusy(null);
        await refreshStatus();
      }
    },
    [active, refreshStatus],
  );

  const startDownload = useCallback(async () => {
    setError(null);
    setProgress({ downloaded: 0, total: model?.spec.size ?? 0 });
    try {
      await downloadModel();
    } catch (e) {
      const message = errorMessage(e);
      // Cancelling is the user's own choice, not a failure.
      if (!message.includes("cancelled")) setError(message);
      setProgress(null);
      return;
    }
    setProgress(null);
    setNeedsModel(false);
    setModel((m) => (m ? { ...m, installed: true } : m));
    // Carry on with what the user wanted before the download interrupted it.
    const speaker = waitingForModel.current;
    waitingForModel.current = null;
    if (speaker) await toggle(speaker);
  }, [model, toggle]);

  const openSettings = useCallback(async () => {
    if (!permissionIssue) return;
    try {
      await openPrivacySettings(permissionIssue);
    } catch (e) {
      setError(errorMessage(e));
    }
  }, [permissionIssue]);

  /** Tries the refused source again, once the user has allowed it in the settings. */
  const retryPermission = useCallback(async () => {
    if (permissionIssue) await toggle(permissionIssue === "microphone" ? "me" : "remote");
  }, [permissionIssue, toggle]);

  const cancel = useCallback(() => void cancelDownload(), []);
  const clear = useCallback(() => setLines([]), []);
  const dismissError = useCallback(() => setError(null), []);

  return {
    lines,
    active,
    busy,
    error,
    model,
    needsModel,
    progress,
    latencyMs,
    permissionIssue,
    toggle,
    openSettings,
    retryPermission,
    startDownload,
    cancel,
    clear,
    dismissError,
  };
}
