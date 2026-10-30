import { useState } from "react";
import { errorMessage, startPlay, type PlaySession } from "../tauri";
import { useContents } from "../useContents";
import { useModel } from "../useModel";
import type { Screen } from "./MainApp";

const megabytes = (bytes: number) => Math.round(bytes / 1_000_000);

function lastEdited(updatedAt: number): string | null {
  if (updatedAt <= 0) return null;
  return new Date(updatedAt).toLocaleString("vi-VN", { dateStyle: "short", timeStyle: "short" });
}

/** Two ways to start: read a Content, or Play with AI in an Interview. */
export function Home({ onNavigate }: { onNavigate: (screen: Screen) => void }) {
  const contents = useContents();
  const model = useModel();
  const [starting, setStarting] = useState<"content" | "interview" | null>(null);
  const [error, setError] = useState<string | null>(null);

  const play = async (session: PlaySession) => {
    setError(null);
    setStarting(session.mode);
    try {
      await startPlay(session);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setStarting(null);
    }
  };

  return (
    <section className="mn-screen">
      <p className="mn-lead">Chọn cách bạn muốn bắt đầu.</p>

      {error && (
        <p className="mn-error" role="alert">
          {error}
        </p>
      )}

      <div className="mn-cards">
        <article className="mn-card" aria-labelledby="home-content">
          <header>
            <h2 id="home-content">Content</h2>
            <p>Đọc một nội dung bằng teleprompter.</p>
          </header>

          {contents.loaded && contents.items.length === 0 ? (
            <div className="mn-empty">
              <p>Chưa có nội dung nào.</p>
              <button type="button" className="mn-btn secondary" onClick={() => onNavigate("content")}>
                Tạo ở mục Content
              </button>
            </div>
          ) : (
            <ul className="mn-choices" role="radiogroup" aria-label="Chọn nội dung">
              {contents.items.map((item) => {
                const when = lastEdited(item.updated_at);
                const checked = item.id === contents.selected;
                return (
                  <li key={item.id}>
                    <button
                      type="button"
                      role="radio"
                      aria-checked={checked}
                      className={checked ? "mn-choice is-selected" : "mn-choice"}
                      onClick={() => void contents.select(item.id)}
                    >
                      <span className="mn-choice-title">{item.title}</span>
                      {when && <span className="mn-choice-meta">Sửa lần cuối {when}</span>}
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
          {contents.error && <p className="mn-error">{contents.error}</p>}

          <footer>
            <button
              type="button"
              className="mn-btn primary"
              disabled={!contents.selected || starting !== null}
              onClick={() => contents.selected && void play({ mode: "content", content_id: contents.selected })}
            >
              {starting === "content" ? "Đang mở…" : "Play"}
            </button>
          </footer>
        </article>

        <article className="mn-card" aria-labelledby="home-interview">
          <header>
            <h2 id="home-interview">Interview</h2>
            <p>Nghe cuộc phỏng vấn và hiện phụ đề trực tiếp.</p>
          </header>

          <ul className="mn-checks">
            <li className={model.installed ? "is-ok" : "is-todo"}>
              <span>Mô hình nhận dạng giọng nói</span>
              <span className="mn-check-state">
                {model.installed ? "Đã có" : "Chưa tải"}
              </span>
            </li>
          </ul>

          {!model.installed && !model.progress && (
            <div className="mn-note">
              <p>
                Cần tải mô hình một lần
                {model.info ? ` (khoảng ${megabytes(model.info.spec.size)} MB, giấy phép ${model.info.spec.license})` : ""}
                . Mọi thứ chạy trên máy bạn.
              </p>
              <button type="button" className="mn-btn secondary" onClick={() => void model.download()}>
                Tải mô hình
              </button>
            </div>
          )}
          {model.progress && (
            <div className="mn-note">
              <progress value={model.progress.downloaded} max={model.progress.total || undefined} />
              <p>
                Đang tải {megabytes(model.progress.downloaded)}
                {model.progress.total > 0 ? ` / ${megabytes(model.progress.total)}` : ""} MB
              </p>
              <button type="button" className="mn-btn secondary" onClick={model.cancel}>
                Huỷ
              </button>
            </div>
          )}
          {model.error && <p className="mn-error">{model.error}</p>}

          <p className="mn-fineprint">Trợ lý AI gợi ý câu trả lời sẽ có ở bản sau.</p>

          <footer>
            <button
              type="button"
              className="mn-btn secondary"
              disabled={!model.installed || starting !== null}
              onClick={() => void play({ mode: "interview", content_id: null })}
            >
              {starting === "interview" ? "Đang mở…" : "Nghe và hiện phụ đề"}
            </button>
          </footer>
        </article>
      </div>
    </section>
  );
}
