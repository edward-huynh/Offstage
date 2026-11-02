import { useEffect, useState } from "react";
import { Check, Plus, Trash2 } from "lucide-react";
import { Button } from "@/components/motion/button";
import { Card, CardContent } from "@/components/motion/card";
import { Input } from "@/components/motion/input";
import { Switch } from "@/components/motion/switch";
import { cn } from "@/lib/utils";
import type { Workspace, WorkspaceUpdate } from "../tauri";
import type { Workspaces as WorkspacesState } from "../useWorkspaces";
import { ErrorNote, Row, Screen } from "./ui";

/** How long "delete" waits for a second click before it goes back to normal. */
const CONFIRM_MS = 3000;

/** The providers a workspace can use in place of the app-wide one. The key is the app-wide one. */
const PROVIDERS: { id: string; label: string }[] = [
  { id: "openai", label: "OpenAI" },
  { id: "opencode-zen", label: "OpenCode Zen" },
  { id: "opencode-go", label: "OpenCode Go" },
];

const SELECT_CLASS =
  "h-9 max-w-64 rounded-lg border border-border bg-background px-2.5 text-sm text-foreground";

/**
 * Workspaces: one per situation (an interview, a sales call). Each keeps its own documents, a
 * note for the model, and may keep the call on this machine or use another provider.
 */
export function Workspaces({ state }: { state: WorkspacesState }) {
  const [name, setName] = useState("");
  const [creating, setCreating] = useState(false);

  const create = async () => {
    if (creating || name.trim() === "") return;
    setCreating(true);
    const ok = await state.create(name);
    setCreating(false);
    if (ok) setName("");
  };

  return (
    <Screen>
      <p className="max-w-prose text-sm leading-relaxed text-muted-foreground">
        Mỗi workspace dành cho một tình huống, như một buổi phỏng vấn hay một cuộc gọi bán hàng. Nó có tài liệu riêng
        (mục Knowledge), một ghi chú cho AI, và có thể giữ cuộc gọi chỉ trên máy hoặc dùng nhà cung cấp khác. AI chỉ tìm
        trong tài liệu của workspace đang dùng.
      </p>

      <form
        className="flex flex-wrap items-center gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          void create();
        }}
      >
        <Input
          className="min-w-56 flex-1"
          classNames={{ field: "h-10 rounded-xl", input: "text-sm" }}
          value={name}
          maxLength={80}
          placeholder="Tên workspace mới"
          aria-label="Tên workspace mới"
          onChange={setName}
        />
        <Button type="submit" size="sm" disabled={creating || name.trim() === ""}>
          <Plus className="size-4" />
          Tạo workspace
        </Button>
      </form>

      {state.error && <ErrorNote>{state.error}</ErrorNote>}

      <div className="flex flex-col gap-4">
        {state.workspaces.map((workspace) => (
          <WorkspaceCard
            key={workspace.id}
            workspace={workspace}
            inUse={state.active?.id === workspace.id}
            onlyOne={state.workspaces.length <= 1}
            onUse={() => void state.select(workspace.id)}
            onUpdate={(update) => void state.update(workspace.id, update)}
            onDelete={() => void state.remove(workspace.id)}
          />
        ))}
      </div>
    </Screen>
  );
}

function WorkspaceCard({
  workspace,
  inUse,
  onlyOne,
  onUse,
  onUpdate,
  onDelete,
}: {
  workspace: Workspace;
  inUse: boolean;
  onlyOne: boolean;
  onUse: () => void;
  onUpdate: (update: WorkspaceUpdate) => void;
  onDelete: () => void;
}) {
  // What is being typed, until the field loses focus: a half-typed name is not worth saving.
  const [name, setName] = useState(workspace.name);
  const [note, setNote] = useState(workspace.note);
  const [model, setModel] = useState(workspace.model ?? "");
  const [confirming, setConfirming] = useState(false);

  // The saved values win when they change from outside (the other screen, a trimmed name).
  useEffect(() => setName(workspace.name), [workspace.name]);
  useEffect(() => setNote(workspace.note), [workspace.note]);
  useEffect(() => setModel(workspace.model ?? ""), [workspace.model]);

  useEffect(() => {
    if (!confirming) return;
    const id = window.setTimeout(() => setConfirming(false), CONFIRM_MS);
    return () => window.clearTimeout(id);
  }, [confirming]);

  /** Saves the stored values with some of them replaced. */
  const save = (patch: Partial<WorkspaceUpdate>) =>
    onUpdate({
      name: workspace.name,
      note: workspace.note,
      local_only: workspace.local_only,
      provider_id: workspace.provider_id,
      model: workspace.model,
      ...patch,
    });

  return (
    <Card className={cn("bg-card", inUse && "ring-1 ring-primary/50")}>
      <CardContent className="flex flex-col gap-1 p-4">
        <div className="flex flex-wrap items-center gap-2">
          <Input
            className="min-w-48 flex-1"
            classNames={{ field: "h-10 rounded-xl", input: "text-sm font-medium" }}
            value={name}
            maxLength={80}
            aria-label="Tên workspace"
            onChange={setName}
            onBlur={() => {
              if (name.trim() === "") setName(workspace.name);
              else if (name.trim() !== workspace.name) save({ name });
            }}
          />
          {inUse ? (
            <span className="inline-flex items-center gap-1.5 rounded-full bg-success/10 px-2.5 py-1 text-xs font-semibold text-success">
              <Check className="size-3.5" />
              Đang dùng
            </span>
          ) : (
            <Button variant="secondary" size="sm" onClick={onUse}>
              Dùng workspace này
            </Button>
          )}
          <Button
            variant="outline"
            size="sm"
            disabled={onlyOne}
            title={onlyOne ? "Phải còn ít nhất một workspace" : undefined}
            aria-label={`Xoá ${workspace.name}`}
            className={cn(
              "shrink-0",
              confirming
                ? "border-destructive/60 text-destructive hover:bg-destructive/10"
                : "text-muted-foreground",
            )}
            onClick={() => {
              if (!confirming) setConfirming(true);
              else {
                setConfirming(false);
                onDelete();
              }
            }}
          >
            <Trash2 className="size-3.5" />
            {confirming ? "Xoá cả tài liệu?" : "Xoá"}
          </Button>
        </div>

        <div className="mt-2 flex flex-col gap-1.5">
          <label className="text-sm font-medium text-foreground" htmlFor={`note-${workspace.id}`}>
            Ghi chú cho AI
          </label>
          <span className="text-xs leading-relaxed text-muted-foreground">
            Điều bạn muốn AI biết về tình huống này mà tài liệu không nói, như vị trí ứng tuyển, công ty, người nghe.
            AI dùng nó để hiểu bối cảnh, còn sự thật vẫn lấy từ tài liệu.
          </span>
          <textarea
            id={`note-${workspace.id}`}
            className="mn-scroll min-h-24 resize-y rounded-xl border border-border bg-background p-3 font-sans text-[14px] leading-relaxed text-foreground outline-none transition-colors placeholder:text-muted-foreground/60 focus:border-border-strong"
            value={note}
            maxLength={2000}
            placeholder="Ví dụ: Phỏng vấn vị trí backend Rust ở công ty ABC, người phỏng vấn là trưởng nhóm kỹ thuật."
            spellCheck={false}
            onChange={(e) => setNote(e.target.value)}
            onBlur={() => {
              if (note.trim() !== workspace.note) save({ note });
            }}
          />
        </div>

        <Row
          label="Chỉ chạy trên máy"
          hint="Với workspace này, không gửi lời thoại hay tài liệu đi đâu, kể cả khi cài đặt chung dùng nhà cung cấp qua mạng. Cần model trên máy (Cài đặt, Trợ lý AI)."
        >
          <Switch
            label={`Chỉ chạy trên máy cho ${workspace.name}`}
            checked={workspace.local_only}
            onCheckedChange={(on) => save({ local_only: on })}
          />
        </Row>

        <Row
          label="Nhà cung cấp riêng"
          hint="Dùng nhà cung cấp khác với cài đặt chung. Khoá API vẫn là khoá đã lưu trong Cài đặt."
        >
          <select
            className={SELECT_CLASS}
            aria-label="Nhà cung cấp riêng"
            value={workspace.provider_id ?? ""}
            onChange={(e) => save({ provider_id: e.target.value || null, model: e.target.value ? workspace.model : null })}
          >
            <option value="">Dùng cài đặt chung</option>
            {PROVIDERS.map((p) => (
              <option key={p.id} value={p.id}>
                {p.label}
              </option>
            ))}
          </select>
          {workspace.provider_id && (
            <Input
              className="w-48"
              aria-label="Model của nhà cung cấp riêng"
              placeholder="Tên model"
              value={model}
              onChange={setModel}
              onBlur={() => {
                if (model.trim() !== (workspace.model ?? "")) save({ model });
              }}
            />
          )}
        </Row>
      </CardContent>
    </Card>
  );
}
