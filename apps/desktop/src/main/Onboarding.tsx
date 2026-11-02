import { useEffect, useState } from "react";
import { AudioLines, Check, Download, Sparkles } from "lucide-react";
import { Button } from "@/components/motion/button";
import { RadioGroup, RadioGroupItem } from "@/components/motion/radio";
import { setLanguage, useLanguage, useT, type Lang } from "../i18n";
import {
  audioPermissions,
  errorMessage,
  getAssistantConfig,
  getSettings,
  openPrivacySettings,
  requestMicrophoneAccess,
  setAssistantConfig,
  startPlay,
  updateSettings,
  type Access,
} from "../tauri";
import { useContents } from "../useContents";
import { useLocalLlm } from "../useLocalLlm";
import { useModel } from "../useModel";
import { ErrorNote, Progress } from "./ui";

const STEPS = ["welcome", "speech", "permissions", "assistant", "done"] as const;
type Step = (typeof STEPS)[number];

type AssistantChoice = "local" | "cloud" | "later";

const megabytes = (bytes: number) => Math.round(bytes / 1_000_000);
const gigabytes = (bytes: number) => (bytes / 1_000_000_000).toFixed(1).replace(".", ",");

/**
 * The first-run walkthrough: language, the speech model, audio permissions, where the AI runs, and
 * a first script. Every step can be skipped: nothing here is required to use the teleprompter.
 * What it sets up is in Settings too.
 */
export function Onboarding({ onDone, onOpenSettings }: { onDone: () => void; onOpenSettings: () => void }) {
  const t = useT();
  const [index, setIndex] = useState(0);
  const step: Step = STEPS[index];
  const [choice, setChoice] = useState<AssistantChoice>("later");
  const [applying, setApplying] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const local = useLocalLlm();
  const contents = useContents();
  const recommended = local.info?.models.find((m) => m.id === local.info?.recommended);

  // What the AI choice needs before the step can be left.
  const blocked = step === "assistant" && choice === "local" && !recommended?.installed;

  const next = async () => {
    setError(null);
    if (step === "assistant" && choice !== "later") {
      setApplying(true);
      try {
        const config = await getAssistantConfig();
        // Local: on, kept on this computer. Cloud: the key is typed in Settings, so it stays off
        // until then, and the walkthrough ends there.
        if (choice === "local") await setAssistantConfig({ ...config, enabled: true, local_only: true });
      } catch (e) {
        setError(errorMessage(e));
        setApplying(false);
        return;
      }
      setApplying(false);
    }
    setIndex((i) => Math.min(i + 1, STEPS.length - 1));
  };

  const finish = (after?: () => void) => {
    onDone();
    after?.();
  };

  const tryScript = async () => {
    const id = contents.selected ?? contents.items[0]?.id;
    if (!id) return finish();
    try {
      await startPlay({ mode: "content", content_id: id });
      finish();
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex flex-col bg-background" role="dialog" aria-modal="true" aria-label={t("onboarding.welcome.title")}>
      <div className="mn-topbar shrink-0" data-tauri-drag-region />
      <div className="mn-scroll flex min-h-0 flex-1 items-start justify-center overflow-y-auto px-6 pb-10">
        <div className="flex w-full max-w-xl flex-col gap-6 pt-6">
          <div className="flex items-center justify-between gap-3 text-xs text-muted-foreground">
            <span>{t("onboarding.step", { step: index + 1, total: STEPS.length })}</span>
            {step !== "done" && (
              <button type="button" className="underline-offset-2 hover:text-foreground hover:underline" onClick={() => finish()}>
                {t("onboarding.skipAll")}
              </button>
            )}
          </div>
          <div className="flex gap-1.5" aria-hidden>
            {STEPS.map((s, i) => (
              <span key={s} className={i <= index ? "h-1 flex-1 rounded-full bg-primary" : "h-1 flex-1 rounded-full bg-muted"} />
            ))}
          </div>

          {step === "welcome" && <Welcome />}
          {step === "speech" && <Speech />}
          {step === "permissions" && <Permissions />}
          {step === "assistant" && (
            <Assistant choice={choice} onChoice={setChoice} local={local} recommendedId={recommended?.id} />
          )}
          {step === "done" && <Done />}

          {error && <ErrorNote>{error}</ErrorNote>}

          <div className="flex flex-wrap items-center justify-between gap-3">
            {index > 0 && step !== "done" ? (
              <Button variant="ghost" size="sm" onClick={() => setIndex((i) => i - 1)}>
                {t("onboarding.back")}
              </Button>
            ) : (
              <span />
            )}
            {step === "done" ? (
              <div className="flex flex-wrap items-center gap-2">
                <Button variant="secondary" size="sm" onClick={() => void tryScript()} disabled={!contents.loaded}>
                  {t("onboarding.done.try")}
                </Button>
                <Button
                  size="sm"
                  onClick={() => finish(choice === "cloud" ? onOpenSettings : undefined)}
                >
                  {t("onboarding.done.enter")}
                </Button>
              </div>
            ) : (
              <Button size="sm" disabled={blocked || applying} onClick={() => void next()}>
                {t("onboarding.next")}
              </Button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

function Heading({ icon, title, body }: { icon?: React.ReactNode; title: string; body: string }) {
  return (
    <header className="flex flex-col gap-2">
      <div className="flex items-center gap-2.5">
        {icon && <span className="grid size-9 place-items-center rounded-xl bg-muted text-muted-foreground">{icon}</span>}
        <h1 className="text-xl font-semibold text-foreground">{title}</h1>
      </div>
      <p className="text-sm leading-relaxed text-muted-foreground">{body}</p>
    </header>
  );
}

function Welcome() {
  const t = useT();
  const language = useLanguage();
  const choose = async (lang: Lang) => {
    setLanguage(lang);
    try {
      await updateSettings({ ...(await getSettings()), ui_language: lang });
    } catch {
      // The language still changed on screen; saving it is retried by the Settings screen.
    }
  };
  return (
    <>
      <Heading title={t("onboarding.welcome.title")} body={t("onboarding.welcome.body")} />
      <div className="flex flex-col gap-2">
        <span className="text-sm font-medium text-foreground">{t("onboarding.welcome.language")}</span>
        <RadioGroup
          orientation="horizontal"
          value={language}
          onValueChange={(v) => void choose(v as Lang)}
          aria-label={t("onboarding.welcome.language")}
        >
          <RadioGroupItem value="vi" label={t("settings.lang.vi")} />
          <RadioGroupItem value="en" label={t("settings.lang.en")} />
        </RadioGroup>
      </div>
    </>
  );
}

/** A download in progress, or the button that starts it, or the mark that it is done. */
function DownloadStatus({
  installed,
  progress,
  error,
  onDownload,
  onCancel,
}: {
  installed: boolean;
  progress: { downloaded: number; total: number } | null;
  error: string | null;
  onDownload: () => void;
  onCancel: () => void;
}) {
  const t = useT();
  return (
    <div className="flex flex-col gap-2">
      {progress ? (
        <div className="flex flex-col gap-2">
          <Progress value={progress.downloaded} max={progress.total || undefined} />
          <div className="flex items-center justify-between gap-3 text-xs text-muted-foreground">
            <span>
              {megabytes(progress.downloaded)}
              {progress.total > 0 ? ` / ${megabytes(progress.total)}` : ""} MB
            </span>
            <Button variant="ghost" size="sm" onClick={onCancel}>
              {t("common.cancel")}
            </Button>
          </div>
        </div>
      ) : installed ? (
        <span className="inline-flex w-fit items-center gap-1.5 rounded-full bg-success/10 px-2.5 py-1 text-xs font-semibold text-success">
          <Check className="size-3.5" />
          {t("common.downloaded")}
        </span>
      ) : (
        <Button className="w-fit" size="sm" onClick={onDownload}>
          <Download className="size-4" />
          {t("common.download")}
        </Button>
      )}
      {error && <ErrorNote>{error}</ErrorNote>}
    </div>
  );
}

function Speech() {
  const t = useT();
  const model = useModel();
  return (
    <>
      <Heading icon={<AudioLines className="size-4.5" />} title={t("onboarding.speech.title")} body={t("onboarding.speech.body")} />
      {model.info === null ? (
        <p className="text-sm text-muted-foreground">{t("settings.speech.modelUnknown")}</p>
      ) : (
        model.info && (
          <>
            <p className="text-xs text-muted-foreground">
              {t("settings.speech.modelHint", {
                file: model.info.spec.file_name,
                size: megabytes(model.info.spec.size),
                license: model.info.spec.license,
              })}
            </p>
            <DownloadStatus
              installed={model.installed}
              progress={model.progress}
              error={model.error}
              onDownload={() => void model.download()}
              onCancel={model.cancel}
            />
          </>
        )
      )}
    </>
  );
}

function Permissions() {
  const t = useT();
  const [mic, setMic] = useState<Access>("unknown");
  const [asking, setAsking] = useState(false);

  useEffect(() => {
    void audioPermissions().then((p) => setMic(p.microphone));
  }, []);

  const allow = async () => {
    setAsking(true);
    try {
      setMic(await requestMicrophoneAccess());
    } finally {
      setAsking(false);
    }
  };

  const micText =
    mic === "granted"
      ? t("onboarding.permissions.micGranted")
      : mic === "denied"
        ? t("onboarding.permissions.micDenied")
        : mic === "not_determined"
          ? t("onboarding.permissions.micAsk")
          : t("onboarding.permissions.micUnknown");

  return (
    <>
      <Heading title={t("onboarding.permissions.title")} body={t("onboarding.permissions.body")} />
      <div className="flex flex-col gap-2 rounded-xl border border-border bg-card p-4">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <span className="text-sm font-medium text-foreground">{t("onboarding.permissions.mic")}</span>
          <span className={mic === "granted" ? "text-xs font-semibold text-success" : "text-xs text-muted-foreground"}>{micText}</span>
        </div>
        <div className="flex flex-wrap gap-2">
          {mic === "not_determined" && (
            <Button size="sm" disabled={asking} onClick={() => void allow()}>
              {t("onboarding.permissions.allow")}
            </Button>
          )}
          {mic === "denied" && (
            <Button variant="secondary" size="sm" onClick={() => void openPrivacySettings("microphone")}>
              {t("onboarding.permissions.openMic")}
            </Button>
          )}
        </div>
      </div>
      <div className="flex flex-col gap-2 rounded-xl border border-border bg-card p-4">
        <span className="text-sm font-medium text-foreground">{t("onboarding.permissions.system")}</span>
        <p className="text-xs leading-relaxed text-muted-foreground">{t("onboarding.permissions.systemBody")}</p>
        <Button className="w-fit" variant="secondary" size="sm" onClick={() => void openPrivacySettings("system_audio").catch(() => {})}>
          {t("onboarding.permissions.openSystem")}
        </Button>
      </div>
    </>
  );
}

function Assistant({
  choice,
  onChoice,
  local,
  recommendedId,
}: {
  choice: AssistantChoice;
  onChoice: (choice: AssistantChoice) => void;
  local: ReturnType<typeof useLocalLlm>;
  recommendedId: string | undefined;
}) {
  const t = useT();
  const model = local.info?.models.find((m) => m.id === recommendedId);
  return (
    <>
      <Heading icon={<Sparkles className="size-4.5" />} title={t("onboarding.ai.title")} body={t("onboarding.ai.body")} />
      <RadioGroup value={choice} onValueChange={(v) => onChoice(v as AssistantChoice)} aria-label={t("onboarding.ai.title")}>
        <RadioGroupItem value="local" label={t("onboarding.ai.local")} description={t("onboarding.ai.localHint")} />
        <RadioGroupItem value="cloud" label={t("onboarding.ai.cloud")} description={t("onboarding.ai.cloudHint")} />
        <RadioGroupItem value="later" label={t("onboarding.ai.skip")} description={t("onboarding.ai.skipHint")} />
      </RadioGroup>
      {choice === "local" && model && (
        <div className="flex flex-col gap-2 rounded-xl border border-border bg-card p-4">
          <span className="text-sm text-foreground">
            {t("onboarding.ai.model", { label: model.label, size: gigabytes(model.size) })}
          </span>
          <DownloadStatus
            installed={model.installed}
            progress={local.progress?.id === model.id ? local.progress : null}
            error={local.error}
            onDownload={() => void local.download(model.id, model.size)}
            onCancel={local.cancel}
          />
          {!model.installed && <p className="text-xs text-muted-foreground">{t("onboarding.ai.needModel")}</p>}
        </div>
      )}
    </>
  );
}

function Done() {
  const t = useT();
  return <Heading title={t("onboarding.done.title")} body={t("onboarding.done.body")} />;
}
