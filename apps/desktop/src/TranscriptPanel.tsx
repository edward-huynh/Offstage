import { useEffect, useRef } from "react";
import type { DownloadProgress, Line } from "./useListening";
import type { ModelInfo, Speaker } from "./tauri";

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
  onToggle: (speaker: Speaker) => void;
  onDownload: () => void;
  onCancel: () => void;
  onClear: () => void;
  onDismissError: () => void;
};

const LABEL: Record<Speaker, string> = { me: "Tôi", remote: "Đối phương" };

const megabytes = (bytes: number) => Math.round(bytes / 1_000_000);

/** Live captions at the bottom of the overlay, over the script, so the script never moves. */
export function TranscriptPanel({
  lines,
  active,
  busy,
  error,
  model,
  needsModel,
  progress,
  latencyMs,
  onToggle,
  onDownload,
  onCancel,
  onClear,
  onDismissError,
}: Props) {
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
      {busy === speaker ? "Đang chuẩn bị…" : name} <kbd>{key}</kbd>
    </button>
  );

  return (
    <section className="transcript" aria-label="Phụ đề trực tiếp">
      <div className="tr-bar">
        {source("me", "Mic", "1")}
        {source("remote", "Âm thanh hệ thống", "2")}
        <span className="spacer" />
        {latencyMs !== null && (
          <span className="tr-latency" title="Từ lúc câu nói kết thúc đến lúc có chữ, chưa tính khoảng lặng chờ cuối câu">
            {(latencyMs / 1000).toFixed(1)} s
          </span>
        )}
        <button type="button" onClick={onClear} disabled={lines.length === 0}>
          Xoá
        </button>
      </div>

      {progress ? (
        <div className="tr-notice">
          <progress value={progress.downloaded} max={progress.total || undefined} />
          <span>
            Đang tải mô hình {megabytes(progress.downloaded)}
            {progress.total > 0 ? ` / ${megabytes(progress.total)}` : ""} MB
          </span>
          <button type="button" onClick={onCancel}>
            Huỷ
          </button>
        </div>
      ) : (
        needsModel && (
          <div className="tr-notice">
            <span>
              Cần tải mô hình nhận dạng giọng nói một lần
              {model ? ` (khoảng ${megabytes(model.spec.size)} MB, giấy phép ${model.spec.license})` : ""}. Mọi thứ
              chạy trên máy bạn.
            </span>
            <button type="button" onClick={onDownload}>
              Tải mô hình
            </button>
          </div>
        )
      )}

      {error && (
        <p className="tr-error" role="alert">
          {error} <button type="button" onClick={onDismissError}>Đóng</button>
        </p>
      )}

      <ol className="tr-lines" ref={list} aria-live="polite">
        {lines.length === 0 ? (
          <li className="empty">Bật mic hoặc âm thanh hệ thống để xem phụ đề ở đây.</li>
        ) : (
          lines.map((line) => (
            <li key={line.id} className={`line ${line.speaker}`}>
              <b>{LABEL[line.speaker]}</b> {line.text}
            </li>
          ))
        )}
      </ol>
    </section>
  );
}
