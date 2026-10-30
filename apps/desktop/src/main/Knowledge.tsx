import { useEffect, useRef, useState } from "react";
import { type ImportReport, type KnowledgeKind } from "../tauri";
import { useKnowledge } from "../useKnowledge";

/** Formats a document can be written in, instead of imported from a file. */
type WrittenKind = Extract<KnowledgeKind, "markdown" | "text">;

const FORMATS: { value: WrittenKind; label: string }[] = [
  { value: "markdown", label: "Markdown (.md)" },
  { value: "text", label: "Văn bản (.txt)" },
];

/** How long "delete" waits for a second click before it goes back to normal. */
const CONFIRM_MS = 3000;

const KIND_LABEL: Record<KnowledgeKind, string> = {
  pdf: "PDF",
  docx: "DOCX",
  markdown: "MD",
  text: "TXT",
};

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function formatDate(ms: number): string | null {
  if (ms <= 1000) return null;
  return new Date(ms).toLocaleDateString("vi-VN", { dateStyle: "short" });
}

/** The user's own documents (JD, CV, rubric...). Stored for the AI to read in a later version. */
export function Knowledge() {
  const knowledge = useKnowledge();
  const [report, setReport] = useState<ImportReport | null>(null);
  const [confirming, setConfirming] = useState<string | null>(null);
  const [writing, setWriting] = useState(false);
  const [saving, setSaving] = useState(false);
  const [kind, setKind] = useState<WrittenKind>("markdown");
  const [title, setTitle] = useState("");
  const [text, setText] = useState("");
  const titleRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (writing) titleRef.current?.focus();
  }, [writing]);

  useEffect(() => {
    if (!confirming) return;
    const id = window.setTimeout(() => setConfirming(null), CONFIRM_MS);
    return () => window.clearTimeout(id);
  }, [confirming]);

  const add = async () => {
    setReport(null);
    const result = await knowledge.importFiles();
    if (result && (result.imported.length > 0 || result.skipped.length > 0)) setReport(result);
  };

  const openForm = () => {
    setReport(null);
    knowledge.clearError();
    setWriting(true);
  };

  const closeForm = () => {
    setWriting(false);
    setTitle("");
    setText("");
  };

  const save = async () => {
    if (saving) return;
    setSaving(true);
    const saved = await knowledge.create(title, kind, text);
    setSaving(false);
    // On failure the form stays open with what was typed, so nothing is lost.
    if (saved) closeForm();
  };

  const remove = async (id: string) => {
    if (confirming !== id) {
      setConfirming(id);
      return;
    }
    setConfirming(null);
    await knowledge.remove(id);
  };

  return (
    <section className="mn-screen">
      <div className="mn-know-head">
        <p className="mn-lead">
          Tài liệu của bạn, như mô tả công việc, CV hay tiêu chí đánh giá. Thêm tệp .pdf, .docx, .md, .txt (tối đa 20 MB mỗi tệp)
          hoặc tự gõ, dán nội dung vào.
        </p>
        <div className="mn-know-actions">
          <button type="button" className="mn-btn secondary" disabled={writing} onClick={() => openForm()}>
            Tạo tài liệu
          </button>
          <button type="button" className="mn-btn primary" disabled={knowledge.importing} onClick={() => void add()}>
            {knowledge.importing ? "Đang thêm…" : "Thêm tệp"}
          </button>
        </div>
      </div>

      <p className="mn-note-line">AI chưa đọc tài liệu: tính năng này đến ở bản sau. Hiện tại tài liệu chỉ được lưu trên máy bạn.</p>

      {knowledge.error && (
        <p className="mn-error" role="alert">
          {knowledge.error}
        </p>
      )}

      {writing && (
        <form
          className="mn-know-form"
          onSubmit={(e) => {
            e.preventDefault();
            void save();
          }}
        >
          <div className="mn-know-form-bar">
            <div className="mn-seg" role="radiogroup" aria-label="Định dạng">
              {FORMATS.map((f) => (
                <button
                  key={f.value}
                  type="button"
                  role="radio"
                  aria-checked={kind === f.value}
                  className={kind === f.value ? "mn-seg-item is-selected" : "mn-seg-item"}
                  onClick={() => setKind(f.value)}
                >
                  {f.label}
                </button>
              ))}
            </div>
            <input
              ref={titleRef}
              className="mn-know-title"
              value={title}
              maxLength={100}
              placeholder="Tên tài liệu"
              aria-label="Tên tài liệu"
              onChange={(e) => setTitle(e.target.value)}
            />
          </div>
          <textarea
            className="mn-textarea mn-know-text"
            value={text}
            placeholder={kind === "markdown" ? "Gõ hoặc dán nội dung Markdown…" : "Gõ hoặc dán nội dung…"}
            aria-label="Nội dung"
            spellCheck={false}
            onChange={(e) => setText(e.target.value)}
          />
          <div className="mn-know-form-foot">
            <span>{text.trim() === "" ? "Chưa có nội dung" : formatSize(new TextEncoder().encode(text).length)}</span>
            <span className="mn-know-form-btns">
              <button type="button" className="mn-btn secondary" disabled={saving} onClick={() => closeForm()}>
                Huỷ
              </button>
              <button type="submit" className="mn-btn primary" disabled={saving || text.trim() === ""}>
                {saving ? "Đang lưu…" : "Lưu tài liệu"}
              </button>
            </span>
          </div>
        </form>
      )}

      {report && (
        <div className="mn-note mn-know-report" role="status">
          {report.imported.length > 0 && <p>Đã thêm {report.imported.length} tài liệu.</p>}
          {report.skipped.length > 0 && (
            <>
              <p>Bỏ qua {report.skipped.length} tệp:</p>
              <ul>
                {report.skipped.map((s, i) => (
                  <li key={`${s.name}-${i}`}>
                    <strong>{s.name}</strong>: {s.reason}
                  </li>
                ))}
              </ul>
            </>
          )}
        </div>
      )}

      {knowledge.docs.length === 0 ? (
        <div className="mn-empty big mn-know-empty">
          <p>Chưa có tài liệu nào.</p>
          <button type="button" className="mn-btn secondary" disabled={knowledge.importing} onClick={() => void add()}>
            Chọn tệp để thêm
          </button>
        </div>
      ) : (
        <ul className="mn-know-list">
          {knowledge.docs.map((doc) => {
            const date = formatDate(doc.added_at);
            const asking = confirming === doc.id;
            return (
              <li key={doc.id} className="mn-know-item">
                <span className="mn-know-kind">{KIND_LABEL[doc.kind]}</span>
                <span className="mn-know-name" title={doc.name}>
                  {doc.name}
                </span>
                <span className="mn-know-meta">
                  {formatSize(doc.size_bytes)}
                  {date ? ` · ${date}` : ""}
                </span>
                <button
                  type="button"
                  className={asking ? "mn-btn danger" : "mn-btn secondary"}
                  aria-label={`Xoá ${doc.name}`}
                  onClick={() => void remove(doc.id)}
                >
                  {asking ? "Chắc chắn xoá?" : "Xoá"}
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}
