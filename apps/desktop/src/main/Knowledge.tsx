import { useEffect, useState } from "react";
import { type ImportReport, type KnowledgeKind } from "../tauri";
import { useKnowledge } from "../useKnowledge";

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
          Tài liệu của bạn, như mô tả công việc, CV hay tiêu chí đánh giá. Nhận .pdf, .docx, .md và .txt, tối đa 20 MB mỗi tệp.
        </p>
        <button type="button" className="mn-btn primary" disabled={knowledge.importing} onClick={() => void add()}>
          {knowledge.importing ? "Đang thêm…" : "Thêm tài liệu"}
        </button>
      </div>

      <p className="mn-note-line">AI chưa đọc tài liệu: tính năng này đến ở bản sau. Hiện tại tệp chỉ được lưu trên máy bạn.</p>

      {knowledge.error && (
        <p className="mn-error" role="alert">
          {knowledge.error}
        </p>
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
