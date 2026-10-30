import { useState } from "react";
import { AudioLines, Check, FileText, Sparkles } from "lucide-react";
import { Button } from "@/components/motion/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/motion/card";
import { RadioGroup, RadioGroupItem } from "@/components/motion/radio";
import { ScrollArea } from "@/components/ui/scroll-area";
import { errorMessage, startPlay, type PlaySession } from "../tauri";
import { useContents } from "../useContents";
import { useModel } from "../useModel";
import type { Screen } from "./MainApp";
import { EmptyState, ErrorNote, Progress, Screen as ScreenFrame } from "./ui";

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
    <ScreenFrame className="gap-5">
      <p className="text-sm text-muted-foreground">Chọn cách bạn muốn bắt đầu.</p>

      {error && <ErrorNote>{error}</ErrorNote>}

      <div className="grid items-stretch gap-5 md:grid-cols-2">
        <Card className="bg-card">
          <CardHeader>
            <div className="flex items-center gap-2.5">
              <span className="grid size-9 place-items-center rounded-xl bg-primary/12 text-primary">
                <FileText className="size-4.5" />
              </span>
              <div>
                <CardTitle as="h2">Content</CardTitle>
                <CardDescription>Đọc một nội dung bằng teleprompter.</CardDescription>
              </div>
            </div>
          </CardHeader>

          <CardContent className="flex-1">
            {!contents.loaded ? (
              <p className="text-sm text-muted-foreground">Đang tải nội dung…</p>
            ) : contents.items.length === 0 ? (
              <EmptyState
                action={
                  <Button variant="secondary" size="sm" onClick={() => onNavigate("content")}>
                    Tạo ở mục Content
                  </Button>
                }
              >
                Chưa có nội dung nào.
              </EmptyState>
            ) : (
              <ScrollArea className="max-h-72">
                <RadioGroup
                  value={contents.selected ?? undefined}
                  onValueChange={(id) => void contents.select(id)}
                  aria-label="Chọn nội dung"
                >
                  {contents.items.map((item) => {
                    const when = lastEdited(item.updated_at);
                    return (
                      <RadioGroupItem
                        key={item.id}
                        value={item.id}
                        label={item.title}
                        description={when ? `Sửa lần cuối ${when}` : undefined}
                      />
                    );
                  })}
                </RadioGroup>
              </ScrollArea>
            )}
            {contents.error && <ErrorNote className="mt-3">{contents.error}</ErrorNote>}
          </CardContent>

          <CardFooter className="justify-end">
            <Button
              size="sm"
              disabled={!contents.selected || starting !== null}
              onClick={() =>
                contents.selected && void play({ mode: "content", content_id: contents.selected })
              }
            >
              {starting === "content" ? "Đang mở…" : "Play"}
            </Button>
          </CardFooter>
        </Card>

        <Card className="bg-card">
          <CardHeader>
            <div className="flex items-center gap-2.5">
              <span className="grid size-9 place-items-center rounded-xl bg-info/12 text-info">
                <AudioLines className="size-4.5" />
              </span>
              <div>
                <CardTitle as="h2">Interview</CardTitle>
                <CardDescription>Nghe cuộc phỏng vấn và hiện phụ đề trực tiếp.</CardDescription>
              </div>
            </div>
          </CardHeader>

          <CardContent className="flex flex-1 flex-col gap-3">
            <div className="flex items-center justify-between gap-3 rounded-xl border border-border bg-background/40 px-3.5 py-3">
              <span className="text-sm">Mô hình nhận dạng giọng nói</span>
              <span
                className={
                  model.installed
                    ? "inline-flex items-center gap-1.5 text-xs font-semibold text-success"
                    : "text-xs font-semibold text-primary"
                }
              >
                {model.installed && <Check className="size-3.5" />}
                {model.installed ? "Đã có" : "Chưa tải"}
              </span>
            </div>

            {!model.installed && !model.progress && (
              <div className="flex flex-col items-start gap-2.5 rounded-xl border border-dashed border-border p-3.5">
                <p className="text-sm leading-relaxed text-muted-foreground">
                  Cần tải mô hình một lần
                  {model.info
                    ? ` (khoảng ${megabytes(model.info.spec.size)} MB, giấy phép ${model.info.spec.license})`
                    : ""}
                  . Mọi thứ chạy trên máy bạn.
                </p>
                <Button variant="secondary" size="sm" onClick={() => void model.download()}>
                  Tải mô hình
                </Button>
              </div>
            )}

            {model.progress && (
              <div className="flex flex-col gap-2 rounded-xl border border-border p-3.5">
                <Progress value={model.progress.downloaded} max={model.progress.total || undefined} />
                <div className="flex items-center justify-between gap-3 text-xs text-muted-foreground">
                  <span>
                    Đang tải {megabytes(model.progress.downloaded)}
                    {model.progress.total > 0 ? ` / ${megabytes(model.progress.total)}` : ""} MB
                  </span>
                  <Button variant="ghost" size="sm" onClick={model.cancel}>
                    Huỷ
                  </Button>
                </div>
              </div>
            )}

            {model.error && <ErrorNote>{model.error}</ErrorNote>}

            <p className="mt-auto flex items-start gap-2 text-xs leading-relaxed text-muted-foreground">
              <Sparkles className="mt-0.5 size-3.5 shrink-0 text-primary" />
              Trợ lý AI gợi ý câu trả lời sẽ có ở bản sau.
            </p>
          </CardContent>

          <CardFooter className="justify-end">
            <Button
              variant="secondary"
              size="sm"
              disabled={!model.installed || starting !== null}
              onClick={() => void play({ mode: "interview", content_id: null })}
            >
              {starting === "interview" ? "Đang mở…" : "Nghe và hiện phụ đề"}
            </Button>
          </CardFooter>
        </Card>
      </div>
    </ScreenFrame>
  );
}
