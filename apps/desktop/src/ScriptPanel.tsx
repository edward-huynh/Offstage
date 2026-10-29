import { useEffect, useRef, useState } from "react";
import type { ScriptMeta } from "./tauri";

type Props = {
  scripts: ScriptMeta[];
  activeId: string | null;
  error: string | null;
  onSelect: (id: string) => void;
  onCreate: () => void;
  onImport: () => void;
  onRename: (id: string, title: string) => void;
  onRemove: (id: string) => void;
  onClose: () => void;
};

/** How long "delete" waits for a second click before it goes back to normal. */
const CONFIRM_MS = 3000;

export function ScriptPanel({
  scripts,
  activeId,
  error,
  onSelect,
  onCreate,
  onImport,
  onRename,
  onRemove,
  onClose,
}: Props) {
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [confirmId, setConfirmId] = useState<string | null>(null);

  // The webview has no reliable confirm dialog, so deleting takes two clicks.
  useEffect(() => {
    if (!confirmId) return;
    const id = window.setTimeout(() => setConfirmId(null), CONFIRM_MS);
    return () => window.clearTimeout(id);
  }, [confirmId]);

  // Enter, Escape and losing focus can all end a rename, and unmounting the input may fire
  // a blur after Escape. The flag makes sure each rename is settled once.
  const settled = useRef(true);

  const startRename = (script: ScriptMeta) => {
    setConfirmId(null);
    settled.current = false;
    setRenamingId(script.id);
    setDraft(script.title);
  };

  const finishRename = (save: boolean) => {
    if (settled.current) return;
    settled.current = true;
    if (save && renamingId) onRename(renamingId, draft);
    setRenamingId(null);
  };

  return (
    <section className="panel" aria-label="Danh sách kịch bản">
      <div className="panel-bar">
        <strong>Kịch bản</strong>
        <span className="spacer" />
        <button type="button" onClick={onCreate}>
          Mới
        </button>
        <button type="button" onClick={onImport}>
          Nhập tệp
        </button>
        <button type="button" onClick={onClose}>
          Đóng (Esc)
        </button>
      </div>
      {error && (
        <p className="panel-error" role="alert">
          {error}
        </p>
      )}
      <ul className="panel-list">
        {scripts.map((s) => (
          <li key={s.id} className={s.id === activeId ? "row active" : "row"}>
            {renamingId === s.id ? (
              <input
                className="rename"
                value={draft}
                autoFocus
                aria-label="Tên kịch bản"
                onChange={(e) => setDraft(e.target.value)}
                onBlur={() => finishRename(true)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") finishRename(true);
                  if (e.key === "Escape") finishRename(false);
                }}
              />
            ) : (
              <button
                type="button"
                className="pick"
                aria-current={s.id === activeId ? "true" : undefined}
                onClick={() => onSelect(s.id)}
              >
                {s.title}
              </button>
            )}
            <button type="button" onClick={() => startRename(s)}>
              Đổi tên
            </button>
            <button
              type="button"
              className={confirmId === s.id ? "danger" : undefined}
              onClick={() => {
                if (confirmId === s.id) {
                  setConfirmId(null);
                  onRemove(s.id);
                } else {
                  setConfirmId(s.id);
                }
              }}
            >
              {confirmId === s.id ? "Chắc chắn xoá?" : "Xoá"}
            </button>
          </li>
        ))}
      </ul>
    </section>
  );
}
