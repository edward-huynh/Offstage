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
import { errorMessage, startPlay, type AssistantStatus, type PlaySession } from "../tauri";
import { useAssistantStatus } from "../useAssistant";
import { useContents } from "../useContents";
import { useT, locale, type Key } from "../i18n";
import { useModel } from "../useModel";
import type { Screen } from "./MainApp";
import { EmptyState, ErrorNote, Progress, Screen as ScreenFrame } from "./ui";

const megabytes = (bytes: number) => Math.round(bytes / 1_000_000);

function lastEdited(updatedAt: number): string | null {
  if (updatedAt <= 0) return null;
  return new Date(updatedAt).toLocaleString(locale(), { dateStyle: "short", timeStyle: "short" });
}

/** The key of the line on whether the assistant will suggest answers. */
function assistantNote(status: AssistantStatus | null): Key {
  if (!status) return "home.assistant.default";
  if (status.ready) {
    if (status.local_only) return "home.assistant.localOnly";
    return status.cloud ? "home.assistant.cloud" : "home.assistant.local";
  }
  if (status.enabled) return "home.assistant.notReady";
  return "home.assistant.off";
}

/** Two ways to start: read a Content, or Play with AI in an Interview. */
export function Home({ onNavigate }: { onNavigate: (screen: Screen) => void }) {
  const t = useT();
  const contents = useContents();  const model = useModel();
  const assistant = useAssistantStatus();
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
      <p className="text-sm text-muted-foreground">{t("home.choose")}</p>

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
                <CardDescription>{t("home.content.description")}</CardDescription>
              </div>
            </div>
          </CardHeader>

          <CardContent className="flex-1">
            {!contents.loaded ? (
              <p className="text-sm text-muted-foreground">{t("home.content.loading")}</p>
            ) : contents.items.length === 0 ? (
              <EmptyState
                action={
                  <Button variant="secondary" size="sm" onClick={() => onNavigate("content")}>
                    {t("home.content.create")}
                  </Button>
                }
              >
                {t("home.content.empty")}
              </EmptyState>
            ) : (
              <ScrollArea className="max-h-72">
                <RadioGroup
                  value={contents.selected ?? undefined}
                  onValueChange={(id) => void contents.select(id)}
                  aria-label={t("home.content.pick")}
                >
                  {contents.items.map((item) => {
                    const when = lastEdited(item.updated_at);
                    return (
                      <RadioGroupItem
                        key={item.id}
                        value={item.id}
                        label={item.title}
                        description={when ? t("home.content.lastEdited", { when }) : undefined}
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
              {starting === "content" ? t("home.opening") : "Play"}
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
                <CardDescription>{t("home.interview.description")}</CardDescription>
              </div>
            </div>
          </CardHeader>

          <CardContent className="flex flex-1 flex-col gap-3">
            <div className="flex items-center justify-between gap-3 rounded-xl border border-border bg-background/40 px-3.5 py-3">
              <span className="text-sm">{t("home.model.label")}</span>
              <span
                className={
                  model.installed
                    ? "inline-flex items-center gap-1.5 text-xs font-semibold text-success"
                    : "text-xs font-semibold text-primary"
                }
              >
                {model.installed && <Check className="size-3.5" />}
                {model.installed ? t("home.model.installed") : t("home.model.missing")}
              </span>
            </div>

            {!model.installed && !model.progress && (
              <div className="flex flex-col items-start gap-2.5 rounded-xl border border-dashed border-border p-3.5">
                <p className="text-sm leading-relaxed text-muted-foreground">
                  {t("home.model.needed", {
                    details: model.info
                      ? t("home.model.details", {
                          size: megabytes(model.info.spec.size),
                          license: model.info.spec.license,
                        })
                      : "",
                  })}
                </p>
                <Button variant="secondary" size="sm" onClick={() => void model.download()}>
                  {t("home.model.download")}
                </Button>
              </div>
            )}

            {model.progress && (
              <div className="flex flex-col gap-2 rounded-xl border border-border p-3.5">
                <Progress value={model.progress.downloaded} max={model.progress.total || undefined} />
                <div className="flex items-center justify-between gap-3 text-xs text-muted-foreground">
                  <span>
                    {t("home.model.downloading", {
                      done: megabytes(model.progress.downloaded),
                      of: model.progress.total > 0 ? ` / ${megabytes(model.progress.total)}` : "",
                    })}
                  </span>
                  <Button variant="ghost" size="sm" onClick={model.cancel}>
                    {t("common.cancel")}
                  </Button>
                </div>
              </div>
            )}

            {model.error && <ErrorNote>{model.error}</ErrorNote>}

            <p className="mt-auto flex items-start gap-2 text-xs leading-relaxed text-muted-foreground">
              <Sparkles className="mt-0.5 size-3.5 shrink-0 text-primary" />
              <span>
                {t(assistantNote(assistant))}{" "}
                <button
                  type="button"
                  className="font-medium text-primary underline-offset-2 hover:underline"
                  onClick={() => onNavigate("settings")}
                >
                  {t("home.openSettings")}
                </button>
              </span>
            </p>
          </CardContent>

          <CardFooter className="justify-end">
            <Button
              variant="secondary"
              size="sm"
              disabled={!model.installed || starting !== null}
              onClick={() => void play({ mode: "interview", content_id: null })}
            >
              {starting === "interview" ? t("home.opening") : t("home.listen")}
            </Button>
          </CardFooter>
        </Card>
      </div>
    </ScreenFrame>
  );
}
