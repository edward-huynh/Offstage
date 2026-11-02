import { memo } from "react";
import { t, useT } from "./i18n";
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
  if (!status || !status.enabled) return t("answer.hint.off");
  if (!status.ready) return t("answer.hint.notReady");
  return t("answer.hint.listening");
}

/** The suggested answer in interview mode, written as the model streams it. */
export const AnswerPanel = memo(function AnswerPanel({ answer, status, captionsOpen }: Props) {
  useT();
  const lines = bullets(answer.text);
  const cloud = status?.enabled && status.cloud;
  const onThisMachine = status?.enabled && status.local_only;

  return (
    <section className={captionsOpen ? "assist with-captions" : "assist"} aria-label={t("answer.label")}>
      <header>
        <strong>Play with AI</strong>
        {cloud && (
          <span className="cloud" title={t("answer.networkTitle")}>
            {t("answer.network")}
          </span>
        )}
        {onThisMachine && (
          <span className="cloud" title={t("answer.onDeviceTitle")}>
            {t("answer.onDevice")}
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
          {answer.phase === "done" && answer.fellBack && (
            <p className="sources">{t("answer.fellBack")}</p>
          )}
          {answer.phase === "done" && answer.sources.length > 0 && (
            <p className="sources">{t("answer.sources", { sources: answer.sources.join(" · ") })}</p>
          )}
          {answer.phase === "streaming" && lines.length === 0 && <p>{t("answer.thinking")}</p>}
          {answer.phase === "failed" && (
            <p className="failed">{t("answer.failed")}</p>
          )}
        </>
      )}
    </section>
  );
});
