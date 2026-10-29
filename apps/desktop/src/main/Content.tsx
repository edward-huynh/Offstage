import { useEffect, useState } from "react";
import type { Library } from "../useLibrary";

const SAVE_LABEL = { saved: "Đã lưu", saving: "Đang lưu…", unsaved: "Chưa lưu" } as const;
/** How long "delete" waits for a second click before it goes back to normal. */
const CONFIRM_MS = 3000;

const wordCount = (text: string) => text.trim().split(/\s+/).filter(Boolean).length;

function lastEdited(updatedAt: number): string | null {
  if (updatedAt <= 0) return null;
  return new Date(updatedAt).toLocaleString("vi-VN", { dateStyle: "short", timeStyle: "short" });
}

/** Edit the contents (scripts) read by the teleprompter. The list lives in the sidebar; this is the editor. */
export function Content({ library: scripts }: { library: Library }) {
  const { flush } = scripts;

  // Leaving the screen saves what is still waiting for the pause that triggers autosave.
  useEffect(
    () => () => {
      void flush();
    },
    [flush],
  );

  const open = scripts.scripts.find((s) => s.id === scripts.current) ?? null;

  const [title, setTitle] = useState("");
  useEffect(() => setTitle(open?.title ?? ""), [open?.id, open?.title]);
  const commitTitle = () => {
    if (open && title.trim() !== open.title) void scripts.rename(open.id, title);
  };

  const [confirming, setConfirming] = useState(false);
  useEffect(() => setConfirming(false), [open?.id]);
  useEffect(() => {
    if (!confirming) return;
    const id = window.setTimeout(() => setConfirming(false), CONFIRM_MS);
    return () => window.clearTimeout(id);
  }, [confirming]);

  const empty = scripts.ready && scripts.scripts.length === 0;
  const edited = open ? lastEdited(open.updated_at) : null;

  return (
    <section className="mn-screen fill">
      {scripts.error && (
        <p className="mn-error" role="alert">
          {scripts.error}
        </p>
      )}

      {empty ? (
        <div className="mn-empty big">
          <p>Chưa có nội dung nào.</p>
          <div className="mn-actions">
            <button type="button" className="mn-btn primary" onClick={() => void scripts.create()}>
              Content mới
            </button>
            <button type="button" className="mn-btn secondary" onClick={() => void scripts.importFile()}>
              Nhập tệp .txt hoặc .md
            </button>
          </div>
        </div>
      ) : open ? (
        <div className="mn-editor-pane">
          <div className="mn-editor-bar">
            <input
              className="mn-title-input"
              value={title}
              aria-label="Tên nội dung"
              onChange={(e) => setTitle(e.target.value)}
              onBlur={commitTitle}
              onKeyDown={(e) => {
                if (e.key === "Enter") e.currentTarget.blur();
                if (e.key === "Escape") setTitle(open.title);
              }}
            />
            <button
              type="button"
              className={confirming ? "mn-btn danger" : "mn-btn secondary"}
              onClick={() => {
                if (confirming) void scripts.remove(open.id);
                else setConfirming(true);
              }}
            >
              {confirming ? "Chắc chắn xoá?" : "Xoá"}
            </button>
          </div>
          <textarea
            className="mn-textarea"
            value={scripts.body}
            aria-label="Nội dung"
            placeholder="Gõ hoặc dán nội dung. Ngắt đoạn bằng một dòng trống: mỗi đoạn thành một khối khi đọc."
            onChange={(e) => scripts.edit(e.target.value)}
          />
          <div className="mn-editor-foot">
            <span>
              {wordCount(scripts.body)} từ{edited ? ` · sửa lần cuối ${edited}` : ""}
            </span>
            <span className={`mn-save ${scripts.saveState}`}>{SAVE_LABEL[scripts.saveState]}</span>
          </div>
        </div>
      ) : (
        <div className="mn-empty big">
          <p>Chọn một nội dung ở thanh bên để sửa.</p>
        </div>
      )}
    </section>
  );
}
