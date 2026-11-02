import { useEffect, useState } from "react";
import { Check, Plus, Trash2 } from "lucide-react";
import { Button } from "@/components/motion/button";
import { Card, CardContent } from "@/components/motion/card";
import { Input } from "@/components/motion/input";
import { Switch } from "@/components/motion/switch";
import { cn } from "@/lib/utils";
import { useT } from "../i18n";
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
  const t = useT();
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
        {t("workspaces.intro")}
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
          placeholder={t("workspaces.newName")}
          aria-label={t("workspaces.newName")}
          onChange={setName}
        />
        <Button type="submit" size="sm" disabled={creating || name.trim() === ""}>
          <Plus className="size-4" />
          {t("workspaces.create")}
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
  const t = useT();
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
            aria-label={t("workspaces.name")}
            onChange={setName}
            onBlur={() => {
              if (name.trim() === "") setName(workspace.name);
              else if (name.trim() !== workspace.name) save({ name });
            }}
          />
          {inUse ? (
            <span className="inline-flex items-center gap-1.5 rounded-full bg-success/10 px-2.5 py-1 text-xs font-semibold text-success">
              <Check className="size-3.5" />
              {t("workspaces.inUse")}
            </span>
          ) : (
            <Button variant="secondary" size="sm" onClick={onUse}>
              {t("workspaces.use")}
            </Button>
          )}
          <Button
            variant="outline"
            size="sm"
            disabled={onlyOne}
            title={onlyOne ? t("workspaces.lastOne") : undefined}
            aria-label={t("knowledge.deleteLabel", { name: workspace.name })}
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
            {confirming ? t("workspaces.confirmDelete") : t("common.delete")}
          </Button>
        </div>

        <div className="mt-2 flex flex-col gap-1.5">
          <label className="text-sm font-medium text-foreground" htmlFor={`note-${workspace.id}`}>
            {t("workspaces.note")}
          </label>
          <span className="text-xs leading-relaxed text-muted-foreground">
            {t("workspaces.noteHint")}
          </span>
          <textarea
            id={`note-${workspace.id}`}
            className="mn-scroll min-h-24 resize-y rounded-xl border border-border bg-background p-3 font-sans text-[14px] leading-relaxed text-foreground outline-none transition-colors placeholder:text-muted-foreground/60 focus:border-border-strong"
            value={note}
            maxLength={2000}
            placeholder={t("workspaces.notePlaceholder")}
            spellCheck={false}
            onChange={(e) => setNote(e.target.value)}
            onBlur={() => {
              if (note.trim() !== workspace.note) save({ note });
            }}
          />
        </div>

        <Row
          label={t("workspaces.localOnly")}
          hint={t("workspaces.localOnlyHint")}
        >
          <Switch
            label={t("workspaces.localOnlyFor", { name: workspace.name })}
            checked={workspace.local_only}
            onCheckedChange={(on) => save({ local_only: on })}
          />
        </Row>

        <Row
          label={t("workspaces.provider")}
          hint={t("workspaces.providerHint")}
        >
          <select
            className={SELECT_CLASS}
            aria-label={t("workspaces.provider")}
            value={workspace.provider_id ?? ""}
            onChange={(e) => save({ provider_id: e.target.value || null, model: e.target.value ? workspace.model : null })}
          >
            <option value="">{t("workspaces.providerDefault")}</option>
            {PROVIDERS.map((p) => (
              <option key={p.id} value={p.id}>
                {p.label}
              </option>
            ))}
          </select>
          {workspace.provider_id && (
            <Input
              className="w-48"
              aria-label={t("workspaces.modelLabel")}
              placeholder={t("workspaces.modelPlaceholder")}
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
