import { memo, useEffect, useRef } from "react";
import { t, useT, type Key } from "./i18n";
import type { DownloadProgress, Line } from "./useListening";
import type { ModelInfo, PrivacyPane, Speaker } from "./tauri";

type Props = {
  lines: Line[];
  active: Record<Speaker, boolean>;
  /** The source that is starting or stopping right now, if any. */
  busy: Speaker | null;
  error: string | null;
  model: ModelInfo | null;
  needsModel: boolean;
  progress: DownloadProgress | null;
  latencyMs: number | null;
  /** The source the OS refused to let the app hear, if the last start failed for that reason. */
  permissionIssue: PrivacyPane | null;
  onToggle: (speaker: Speaker) => void;
  onOpenSettings: () => void;
  onRetryPermission: () => void;
  onDownload: () => void;
  onCancel: () => void;
  onClear: () => void;
  onDismissError: () => void;
};

const LABEL: Record<Speaker, Key> = { me: "captions.me", remote: "captions.remote" };

const PERMISSION_HELP: Record<PrivacyPane, Key> = {
  microphone: "captions.permission.microphone",
  system_audio: "captions.permission.systemAudio",
};

const megabytes = (bytes: number) => Math.round(bytes / 1_000_000);

/**
 * Live captions at the bottom of the overlay, over the script, so the script never moves.
 * Memoised: the overlay re-renders on every script change and download tick, and the panel
 * only has new work when a line or its own state arrives.
 */
export const TranscriptPanel = memo(function TranscriptPanel({
  lines,
  active,
  busy,
  error,
  model,
  needsModel,
  progress,
  latencyMs,
  permissionIssue,
  onToggle,
  onOpenSettings,
  onRetryPermission,
  onDownload,
  onCancel,
  onClear,
  onDismissError,
}: Props) {
  useT();
  const list = useRef<HTMLOListElement>(null);

  // Keep the newest line in view.
  useEffect(() => {
    const el = list.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [lines]);

  const source = (speaker: Speaker, name: string, key: string) => (
    <button
      type="button"
      className={active[speaker] ? "src on" : "src"}
      aria-pressed={active[speaker]}
      disabled={busy !== null}
      onClick={() => onToggle(speaker)}
    >
      <span className="dot" aria-hidden />
      {busy === speaker ? t("captions.preparing") : name} <kbd>{key}</kbd>
    </button>
  );

  return (
    <section className="transcript" aria-label={t("captions.label")}>
      <div className="tr-bar">
        {source("me", t("captions.mic"), "1")}
        {source("remote", t("captions.systemAudio"), "2")}
        <span className="spacer" />
        {latencyMs !== null && (
          <span className="tr-latency" title={t("captions.latency")}>
            {(latencyMs / 1000).toFixed(1)} s
          </span>
        )}
        <button type="button" onClick={onClear} disabled={lines.length === 0}>
          {t("captions.clear")}
        </button>
      </div>

      {progress ? (
        <div className="tr-notice">
          <progress value={progress.downloaded} max={progress.total || undefined} />
          <span>
            {t("captions.downloading", {
              done: megabytes(progress.downloaded),
              of: progress.total > 0 ? ` / ${megabytes(progress.total)}` : "",
            })}
          </span>
          <button type="button" onClick={onCancel}>
            {t("common.cancel")}
          </button>
        </div>
      ) : (
        needsModel && (
          <div className="tr-notice">
            <span>
              {t("captions.needModel", {
                details: model
                  ? t("home.model.details", { size: megabytes(model.spec.size), license: model.spec.license })
                  : "",
              })}
            </span>
            <button type="button" onClick={onDownload}>
              {t("captions.downloadModel")}
            </button>
          </div>
        )
      )}

      {permissionIssue && (
        <div className="tr-notice" role="alert">
          <span>{t(PERMISSION_HELP[permissionIssue])}</span>
          <button type="button" onClick={onOpenSettings}>
            {t("home.openSettings")}
          </button>
          <button type="button" onClick={onRetryPermission}>
            {t("captions.retry")}
          </button>
        </div>
      )}

      {error && (
        <p className="tr-error" role="alert">
          {error} <button type="button" onClick={onDismissError}>{t("captions.dismiss")}</button>
        </p>
      )}

      <ol className="tr-lines" ref={list} aria-live="polite">
        {lines.length === 0 ? (
          <li className="empty">{t("captions.empty")}</li>
        ) : (
          lines.map((line) => (
            <li key={line.id} className={`line ${line.speaker}`}>
              <b>{t(LABEL[line.speaker])}</b> {line.text}
            </li>
          ))
        )}
      </ol>
    </section>
  );
});
