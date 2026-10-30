import { useEffect, useState } from "react";
import { FilePlus2, Trash2, Upload } from "lucide-react";
import { Button } from "@/components/motion/button";
import { Input } from "@/components/motion/input";
import { cn } from "@/lib/utils";
import type { Library } from "../useLibrary";
import { EmptyState, ErrorNote, Screen } from "./ui";

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
    <Screen scroll={false} className="gap-3">
      {scripts.error && <ErrorNote>{scripts.error}</ErrorNote>}

      {empty ? (
        <EmptyState
          className="flex-1"
          action={
            <div className="flex flex-wrap items-center justify-center gap-2">
              <Button onClick={() => void scripts.create()}>
                <FilePlus2 className="size-4" />
                Content mới
              </Button>
              <Button variant="secondary" onClick={() => void scripts.importFile()}>
                <Upload className="size-4" />
                Nhập tệp .txt hoặc .md
              </Button>
            </div>
          }
        >
          Chưa có nội dung nào. Tạo một nội dung mới hoặc nhập tệp để bắt đầu.
        </EmptyState>
      ) : open ? (
        <div className="flex min-h-0 flex-1 flex-col gap-3 rounded-2xl border border-border bg-card p-4">
          <div className="flex items-center gap-2">
            <Input
              className="min-w-0 flex-1"
              classNames={{
                field: "h-12 rounded-xl border-transparent bg-transparent",
                input: "px-2 text-lg font-semibold",
              }}
              value={title}
              aria-label="Tên nội dung"
              onChange={setTitle}
              onBlur={commitTitle}
              onKeyDown={(e) => {
                if (e.key === "Enter") e.currentTarget.blur();
                if (e.key === "Escape") {
                  setTitle(open.title);
                  e.currentTarget.blur();
                }
              }}
            />
            <Button
              variant="outline"
              size="sm"
              className={cn(
                "shrink-0",
                confirming
                  ? "border-destructive/60 text-destructive hover:bg-destructive/10"
                  : "text-muted-foreground",
              )}
              onClick={() => {
                if (confirming) void scripts.remove(open.id);
                else setConfirming(true);
              }}
            >
              <Trash2 className="size-3.5" />
              {confirming ? "Chắc chắn xoá?" : "Xoá"}
            </Button>
          </div>

          <textarea
            className="mn-scroll min-h-0 flex-1 resize-none rounded-xl border border-border bg-background p-4 font-sans text-[15px] leading-relaxed text-foreground outline-none transition-colors placeholder:text-muted-foreground/60 focus:border-border-strong"
            value={scripts.body}
            aria-label="Nội dung"
            placeholder="Gõ hoặc dán nội dung. Ngắt đoạn bằng một dòng trống: mỗi đoạn thành một khối khi đọc."
            onChange={(e) => scripts.edit(e.target.value)}
          />

          <footer className="flex items-center justify-between gap-4 text-xs text-muted-foreground">
            <span>
              {wordCount(scripts.body)} từ{edited ? ` · sửa lần cuối ${edited}` : ""}
            </span>
            <span
              className={cn(
                "font-medium",
                scripts.saveState === "saved" && "text-success",
                scripts.saveState === "unsaved" && "text-primary",
              )}
            >
              {SAVE_LABEL[scripts.saveState]}
            </span>
          </footer>
        </div>
      ) : (
        <EmptyState className="flex-1">Chọn một nội dung ở thanh bên để sửa.</EmptyState>
      )}
    </Screen>
  );
}
