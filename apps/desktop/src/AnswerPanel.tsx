import { memo } from "react";
import type { Answer } from "./useAssistant";
import type { AssistantStatus } from "./tauri";

type Props = {
  answer: Answer;
  status: AssistantStatus | null;
  /** The captions panel covers the bottom of the overlay, so the answer leaves room for it. */
  captionsOpen: boolean;
};

/** The model writes "- " bullets, which the panel draws itself. */
function bullets(text: string): string[] {
  return text
    .split("\n")
    .map((line) => line.replace(/^\s*[-•*]\s*/, "").trim())
    .filter(Boolean);
}

function hint(status: AssistantStatus | null): string {
  if (!status || !status.enabled) return "Bật Trợ lý AI trong Cài đặt để nhận gợi ý trả lời.";
  if (!status.ready) return "Trợ lý AI chưa sẵn sàng: thiếu khoá API hoặc model. Mở Cài đặt để hoàn tất.";
  return "Đang nghe. Khi đối phương hỏi, gợi ý trả lời sẽ hiện ở đây.";
}

/** The suggested answer in interview mode, written as the model streams it. */
export const AnswerPanel = memo(function AnswerPanel({ answer, status, captionsOpen }: Props) {
  const lines = bullets(answer.text);
  const cloud = status?.enabled && status.cloud;

  return (
    <section className={captionsOpen ? "assist with-captions" : "assist"} aria-label="Gợi ý trả lời">
      <header>
        <strong>Play with AI</strong>
        {cloud && (
          <span className="cloud" title="Câu hỏi được gửi tới nhà cung cấp qua mạng">
            ☁ Qua mạng
          </span>
        )}
      </header>

      {answer.phase === "idle" ? (
        <p>{hint(status)}</p>
      ) : (
        <>
          <p className="asked">{answer.question}</p>
          <ul aria-live="polite">
            {lines.map((line, i) => (
              <li key={i}>{line}</li>
            ))}
          </ul>
          {answer.phase === "streaming" && lines.length === 0 && <p>Đang nghĩ…</p>}
          {answer.phase === "failed" && (
            <p className="failed">Không tạo được câu trả lời. Xem lý do ở phụ đề (phím T).</p>
          )}
        </>
      )}
    </section>
  );
});
