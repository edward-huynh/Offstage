import { useEffect, useRef, useState } from "react";
import { Check, Download, FilePlus2, FileUp, Trash2 } from "lucide-react";
import { Button } from "@/components/motion/button";
import { Card, CardContent } from "@/components/motion/card";
import { Input } from "@/components/motion/input";
import { RadioGroup, RadioGroupItem } from "@/components/motion/radio";
import { cn } from "@/lib/utils";
import { type ImportReport, type IndexStatus, type KnowledgeKind } from "../tauri";
import { useEmbeddingModel } from "../useEmbeddingModel";
import { useKnowledge } from "../useKnowledge";
import { EmptyState, ErrorNote, Progress, Screen } from "./ui";

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

const megabytes = (bytes: number) => Math.round(bytes / 1_000_000);

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** What the AI can do with a document: read it, not yet, or never (with the reason in `error`). */
function readState(status: IndexStatus | undefined): { label: string; tone: string } {
  if (!status) return { label: "Đang đọc…", tone: "bg-muted text-muted-foreground" };
  if (status.error) return { label: "Không đọc được", tone: "bg-destructive/10 text-destructive" };
  return { label: "AI đã đọc", tone: "bg-success/10 text-success" };
}

function formatDate(ms: number): string | null {
  if (ms <= 1000) return null;
  return new Date(ms).toLocaleDateString("vi-VN", { dateStyle: "short" });
}

/** The user's own documents (JD, CV, rubric...). The AI reads them to ground its answers. */
export function Knowledge() {
  const knowledge = useKnowledge();
  const embedding = useEmbeddingModel();
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
    <Screen>
      <div className="flex flex-wrap items-start justify-between gap-4">
        <p className="max-w-prose text-sm leading-relaxed text-muted-foreground">
          Tài liệu của bạn, như mô tả công việc, CV hay tiêu chí đánh giá. Thêm tệp .pdf, .docx,
          .md, .txt (tối đa 20 MB mỗi tệp) hoặc tự gõ, dán nội dung vào.
        </p>
        <div className="flex flex-wrap items-center gap-2">
          <Button variant="secondary" size="sm" disabled={writing} onClick={openForm}>
            <FilePlus2 className="size-4" />
            Tạo tài liệu
          </Button>
          <Button size="sm" disabled={knowledge.importing} onClick={() => void add()}>
            <FileUp className="size-4" />
            {knowledge.importing ? "Đang thêm…" : "Thêm tệp"}
          </Button>
        </div>
      </div>

      <p className="text-xs leading-relaxed text-muted-foreground">
        Khi Trợ lý AI bật, đối phương hỏi, app tìm vài đoạn liên quan trong các tài liệu này rồi gửi
        kèm câu hỏi để gợi ý trả lời dựa trên đó. Việc đọc tệp và tìm đoạn diễn ra ngay trên máy
        bạn; chỉ các đoạn được chọn mới được gửi đi nếu bạn dùng nhà cung cấp qua mạng.
      </p>

      {embedding.info && (
        <section
          aria-label="Tìm theo nghĩa"
          className="flex flex-wrap items-center justify-between gap-3 rounded-2xl border border-border bg-card px-4 py-3"
        >
          <div className="min-w-0 flex-1 basis-72">
            <h2 className="text-sm font-semibold">Tìm theo nghĩa</h2>
            <p className="mt-0.5 text-xs leading-relaxed text-muted-foreground">
              {embedding.info.installed
                ? embedding.info.pending_chunks > 0
                  ? `Đang chuẩn bị tài liệu: còn ${embedding.info.pending_chunks} đoạn.`
                  : "Đang bật: AI tìm được đoạn liên quan cả khi câu hỏi không dùng đúng từ trong tài liệu, kể cả khác ngôn ngữ."
                : `Tải một mô hình nhỏ (khoảng ${megabytes(embedding.info.size)} MB, giấy phép ${embedding.info.license}) để AI tìm được đoạn liên quan cả khi câu hỏi không dùng đúng từ trong tài liệu, kể cả khác ngôn ngữ. Mô hình chạy ngay trên máy bạn. Chưa tải thì AI vẫn tìm theo từ.`}
            </p>
            {embedding.error && <ErrorNote className="mt-2">{embedding.error}</ErrorNote>}
          </div>
          {embedding.progress ? (
            <div className="flex w-56 flex-col gap-2">
              <Progress
                value={embedding.progress.downloaded}
                max={embedding.progress.total || undefined}
              />
              <div className="flex items-center justify-between gap-3 text-xs text-muted-foreground">
                <span>
                  {megabytes(embedding.progress.downloaded)}
                  {embedding.progress.total > 0 ? ` / ${megabytes(embedding.progress.total)}` : ""} MB
                </span>
                <Button variant="ghost" size="sm" onClick={embedding.cancel}>
                  Huỷ
                </Button>
              </div>
            </div>
          ) : embedding.info.installed ? (
            <span className="inline-flex items-center gap-1.5 rounded-full bg-success/10 px-2.5 py-1 text-xs font-semibold text-success">
              <Check className="size-3.5" />
              Đã tải
            </span>
          ) : (
            <Button size="sm" onClick={() => void embedding.download()}>
              <Download className="size-4" />
              Tải về
            </Button>
          )}
        </section>
      )}

      {knowledge.error && <ErrorNote>{knowledge.error}</ErrorNote>}

      {writing && (
        <form
          className="flex flex-col gap-3 rounded-2xl border border-border bg-card p-4"
          onSubmit={(e) => {
            e.preventDefault();
            void save();
          }}
        >
          <div className="flex flex-wrap items-center gap-3">
            <RadioGroup
              orientation="horizontal"
              value={kind}
              onValueChange={(value) => setKind(value as WrittenKind)}
              aria-label="Định dạng"
            >
              {FORMATS.map((format) => (
                <RadioGroupItem key={format.value} value={format.value} label={format.label} />
              ))}
            </RadioGroup>
            <Input
              ref={titleRef}
              className="min-w-48 flex-1"
              classNames={{ field: "h-10 rounded-xl", input: "text-sm" }}
              value={title}
              maxLength={100}
              placeholder="Tên tài liệu"
              aria-label="Tên tài liệu"
              onChange={setTitle}
            />
          </div>

          <textarea
            className="mn-scroll min-h-56 resize-none rounded-xl border border-border bg-background p-4 font-sans text-[15px] leading-relaxed text-foreground outline-none transition-colors placeholder:text-muted-foreground/60 focus:border-border-strong"
            value={text}
            placeholder={
              kind === "markdown" ? "Gõ hoặc dán nội dung Markdown…" : "Gõ hoặc dán nội dung…"
            }
            aria-label="Nội dung"
            spellCheck={false}
            onChange={(e) => setText(e.target.value)}
          />

          <div className="flex flex-wrap items-center justify-between gap-3 text-xs text-muted-foreground">
            <span>
              {text.trim() === ""
                ? "Chưa có nội dung"
                : formatSize(new TextEncoder().encode(text).length)}
            </span>
            <div className="flex items-center gap-2">
              <Button type="button" variant="secondary" size="sm" disabled={saving} onClick={closeForm}>
                Huỷ
              </Button>
              <Button type="submit" size="sm" disabled={saving || text.trim() === ""}>
                {saving ? "Đang lưu…" : "Lưu tài liệu"}
              </Button>
            </div>
          </div>
        </form>
      )}

      {report && (
        <div
          role="status"
          className="rounded-xl border border-border bg-muted/50 px-4 py-3 text-sm leading-relaxed text-muted-foreground"
        >
          {report.imported.length > 0 && (
            <p className="text-foreground">Đã thêm {report.imported.length} tài liệu.</p>
          )}
          {report.skipped.length > 0 && (
            <>
              <p>Bỏ qua {report.skipped.length} tệp:</p>
              <ul className="mt-1 list-disc space-y-1 pl-5">
                {report.skipped.map((s, i) => (
                  <li key={`${s.name}-${i}`}>
                    <strong className="font-medium text-foreground">{s.name}</strong>: {s.reason}
                  </li>
                ))}
              </ul>
            </>
          )}
        </div>
      )}

      {knowledge.docs.length === 0 ? (
        <EmptyState
          className="min-h-56"
          action={
            <Button
              variant="secondary"
              size="sm"
              disabled={knowledge.importing}
              onClick={() => void add()}
            >
              Chọn tệp để thêm
            </Button>
          }
        >
          Chưa có tài liệu nào.
        </EmptyState>
      ) : (
        <Card className="bg-card">
          <CardContent className="p-0">
            <ul className="divide-y divide-border">
              {knowledge.docs.map((doc) => {
                const date = formatDate(doc.added_at);
                const asking = confirming === doc.id;
                const state = readState(knowledge.index[doc.id]);
                const failure = knowledge.index[doc.id]?.error;
                return (
                  <li
                    key={doc.id}
                    className="flex items-center gap-3 px-4 py-3 transition-colors hover:bg-muted/40"
                  >
                    <span className="min-w-12 shrink-0 rounded-md bg-muted px-2 py-1 text-center text-[10px] font-bold tracking-wider text-muted-foreground">
                      {KIND_LABEL[doc.kind]}
                    </span>
                    <div className="min-w-0 flex-1">
                      <span className="block truncate text-sm font-medium" title={doc.name}>
                        {doc.name}
                      </span>
                      {failure && (
                        <span className="mt-0.5 block text-xs leading-snug text-destructive">
                          {failure}
                        </span>
                      )}
                    </div>
                    <span
                      className={cn(
                        "hidden shrink-0 rounded-full px-2.5 py-1 text-xs font-semibold sm:inline",
                        state.tone,
                      )}
                    >
                      {state.label}
                    </span>
                    <span className="hidden shrink-0 text-xs text-muted-foreground sm:inline">
                      {formatSize(doc.size_bytes)}
                      {date ? ` · ${date}` : ""}
                    </span>
                    <Button
                      variant="outline"
                      size="sm"
                      aria-label={`Xoá ${doc.name}`}
                      className={cn(
                        "shrink-0",
                        asking
                          ? "border-destructive/60 text-destructive hover:bg-destructive/10"
                          : "text-muted-foreground",
                      )}
                      onClick={() => void remove(doc.id)}
                    >
                      <Trash2 className="size-3.5" />
                      {asking ? "Chắc chắn xoá?" : "Xoá"}
                    </Button>
                  </li>
                );
              })}
            </ul>
          </CardContent>
        </Card>
      )}
    </Screen>
  );
}
