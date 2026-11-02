import { useState } from "react";
import { Check, Cloud, Download, Laptop, Loader2, Sparkles } from "lucide-react";
import { Button } from "@/components/motion/button";
import { Input } from "@/components/motion/input";
import { RadioGroup, RadioGroupItem } from "@/components/motion/radio";
import { Switch } from "@/components/motion/switch";
import { useT } from "../i18n";
import { AUTO_LOCAL_MODEL } from "../tauri";
import { useAssistantSettings } from "../useAssistant";
import { useLocalLlm } from "../useLocalLlm";
import { ErrorNote, Progress, Row, Section } from "./ui";

const CUSTOM = "custom";

const PROVIDERS: { id: string; label: string }[] = [
  { id: "openai", label: "OpenAI" },
  { id: "opencode-zen", label: "OpenCode Zen" },
  { id: "opencode-go", label: "OpenCode Go" },
  { id: CUSTOM, label: "" },
];

const gigabytes = (bytes: number) => (bytes / 1_000_000_000).toFixed(1).replace(".", ",");
const megabytes = (bytes: number) => Math.round(bytes / 1_000_000);
const gibibytes = (bytes: number) => Math.round(bytes / 1024 ** 3);

const SELECT_CLASS =
  "h-9 max-w-64 rounded-lg border border-border bg-background px-2.5 text-sm text-foreground";

/** Turns the assistant on, chooses where its answers come from, and keeps the key. */
export function AssistantSettings() {
  const t = useT();
  const a = useAssistantSettings();
  const local = useLocalLlm();
  const [asking, setAsking] = useState(false);
  const [key, setKey] = useState("");
  // What is being typed, until the field loses focus: a half-typed address is not worth saving.
  const [address, setAddress] = useState<string | null>(null);
  const [model, setModel] = useState<string | null>(null);

  const { config, status } = a;
  if (!config || !status) {
    return (
      <Section title={t("ai.title")} icon={<Sparkles className="size-4.5" />}>
        {a.error ? (
          <ErrorNote className="my-3">{a.error}</ErrorNote>
        ) : (
          <p className="flex items-center gap-2 py-3.5 text-sm text-muted-foreground">
            <Loader2 className="size-4 animate-spin" />
            {t("ai.loading")}
          </p>
        )}
      </Section>
    );
  }

  const providerName = (id: string) => (id === CUSTOM ? t("ai.provider.custom") : PROVIDERS.find((p) => p.id === id)?.label ?? id);
  const providerLabel = providerName(config.provider_id);
  const isCustom = config.provider_id === CUSTOM;
  const modelValue = model ?? config.model ?? "";

  const commitAddress = () => {
    if (address !== null && address !== (config.custom_base_url ?? "")) {
      void a.change({ custom_base_url: address.trim() || null });
    }
    setAddress(null);
  };
  const commitModel = (value: string) => {
    setModel(null);
    if (value.trim() !== (config.model ?? "")) void a.change({ model: value.trim() || null });
  };

  let state: string;
  if (status.ready) {
    state = status.local_only
      ? t("ai.state.localOnly")
      : status.cloud
        ? t("ai.state.cloud", { provider: providerLabel })
        : t("ai.state.local");
  } else if (status.enabled && status.local_only) {
    state = t("ai.state.noLocalModel");
  } else if (status.enabled) {
    state = status.needs_key && !status.has_key ? t("ai.state.noKey") : t("ai.state.noModel");
  } else {
    state = t("ai.state.off");
  }

  return (
    <Section
      title={t("ai.title")}
      hint={t("ai.hint")}
      icon={<Sparkles className="size-4.5" />}
    >
      <Row label={t("ai.enable")} hint={state}>
        {status.local_only && config.enabled && (
          <span
            className="inline-flex items-center gap-1.5 rounded-full bg-muted px-2.5 py-1 text-xs font-semibold text-muted-foreground"
            title={t("ai.badge.localTitle")}
          >
            <Laptop className="size-3.5" />
            {t("ai.badge.local")}
          </span>
        )}
        {status.cloud && config.enabled && (
          <span
            className="inline-flex items-center gap-1.5 rounded-full bg-muted px-2.5 py-1 text-xs font-semibold text-muted-foreground"
            title={t("ai.badge.networkTitle")}
          >
            <Cloud className="size-3.5" />
            {t("ai.badge.network")}
          </span>
        )}
        <Switch
          label={t("ai.enableLabel")}
          checked={config.enabled}
          onCheckedChange={(on) => {
            // Nothing is sent anywhere in local-only mode, so there is nothing to agree to.
            if (on && !config.local_only) setAsking(true);
            else if (on) void a.change({ enabled: true });
            else {
              setAsking(false);
              void a.change({ enabled: false });
            }
          }}
        />
      </Row>

      {asking && !config.enabled && (
        <div className="my-3 flex flex-col gap-3 rounded-xl border border-border bg-muted/50 p-3.5">
          <p className="text-sm leading-relaxed text-foreground">
            {t("ai.consent", { provider: "{provider}" })
              .split("{provider}")
              .flatMap((part, i) => (i === 0 ? [part] : [<strong key="provider">{providerLabel}</strong>, part]))}
          </p>
          <div className="flex justify-end gap-2">
            <Button variant="ghost" size="sm" onClick={() => setAsking(false)}>
              {t("common.cancel")}
            </Button>
            <Button
              size="sm"
              onClick={() => {
                setAsking(false);
                void a.change({ enabled: true });
              }}
            >
              {t("ai.consent.agree")}
            </Button>
          </div>
        </div>
      )}

      <Row
        label={t("ai.localOnly")}
        hint={t("ai.localOnlyHint")}
      >
        <Switch
          label={t("ai.localOnly")}
          checked={config.local_only}
          disabled={!config.local_only && !status.local_installed}
          onCheckedChange={(on) => void a.change({ local_only: on })}
        />
      </Row>

      {!config.local_only && (
        <Row
          label={t("ai.fallback")}
          hint={t("ai.fallbackHint")}
        >
          <Switch
            label={t("ai.fallback")}
            checked={config.fallback_to_local}
            onCheckedChange={(on) => void a.change({ fallback_to_local: on })}
          />
        </Row>
      )}

      <Row
        label={t("ai.localModel")}
        hint={
          local.info
            ? (local.info.ram_bytes > 0
                ? t("ai.localModel.ram", { ram: gibibytes(local.info.ram_bytes) })
                : t("ai.localModel.noRam")) + t("ai.localModel.hint")
            : undefined
        }
      >
        {local.info && (
          <RadioGroup
            orientation="horizontal"
            value={config.local_model}
            onValueChange={(id) => void a.change({ local_model: id })}
            aria-label={t("ai.localModel")}
          >
            <RadioGroupItem
              value={AUTO_LOCAL_MODEL}
              label={t("ai.localModel.auto", {
                model: local.info.models.find((m) => m.id === local.info?.recommended)?.label ?? "…",
              })}
            />
            {local.info.models.map((m) => (
              <RadioGroupItem key={m.id} value={m.id} label={`${m.label} (${gigabytes(m.size)} GB)`} />
            ))}
          </RadioGroup>
        )}
      </Row>

      {local.info && (
        <LocalModelCard
          model={local.info.models.find(
            (m) => m.id === (config.local_model === AUTO_LOCAL_MODEL ? local.info?.recommended : config.local_model),
          )}
          progress={local.progress}
          engineFound={local.info.engine_found}
          error={local.error}
          onDownload={(id, size) => void local.download(id, size)}
          onCancel={local.cancel}
        />
      )}

      {!config.local_only && (
        <>
          <Row label={t("ai.provider")} hint={t("ai.providerHint")}>
            <RadioGroup
              orientation="horizontal"
              value={config.provider_id}
              onValueChange={(id) => void a.changeProvider(id)}
              aria-label={t("ai.provider")}
            >
              {PROVIDERS.map((p) => (
                <RadioGroupItem key={p.id} value={p.id} label={providerName(p.id)} />
              ))}
            </RadioGroup>
          </Row>

          {isCustom && (
            <Row
              label={t("ai.endpoint")}
              hint={t("ai.endpointHint")}
            >
              <Input
                className="w-72"
                aria-label={t("ai.endpoint")}
                placeholder="https://…/v1"
                value={address ?? config.custom_base_url ?? ""}
                onChange={setAddress}
                onBlur={commitAddress}
              />
            </Row>
          )}

          <Row
            label={t("ai.key")}
            hint={t("ai.keyHint")}
          >
            {status.has_key ? (
              <>
                <span className="inline-flex items-center gap-1.5 rounded-full bg-success/10 px-2.5 py-1 text-xs font-semibold text-success">
                  <Check className="size-3.5" />
                  {t("ai.key.saved")}
                </span>
                <Button variant="ghost" size="sm" disabled={a.busy === "key"} onClick={() => void a.removeKey()}>
                  {t("ai.key.remove")}
                </Button>
              </>
            ) : (
              <>
                <Input
                  className="w-56"
                  type="password"
                  autoComplete="off"
                  aria-label={t("ai.key")}
                  placeholder={isCustom ? t("ai.key.optional") : "sk-…"}
                  value={key}
                  onChange={setKey}
                />
                <Button
                  size="sm"
                  disabled={!key.trim() || a.busy === "key"}
                  onClick={() => {
                    void a.saveKey(key).then(() => setKey(""));
                  }}
                >
                  {t("ai.key.save")}
                </Button>
              </>
            )}
          </Row>

          <Row label={t("ai.model")} hint={t("ai.modelHint")}>
            {a.models.length > 0 ? (
              <select
                className={SELECT_CLASS}
                aria-label={t("ai.model")}
                value={modelValue}
                onChange={(e) => commitModel(e.target.value)}
              >
                {modelValue === "" && <option value="">{t("ai.model.choose")}</option>}
                {!a.models.includes(modelValue) && modelValue !== "" && <option value={modelValue}>{modelValue}</option>}
                {a.models.map((m) => (
                  <option key={m} value={m}>
                    {m}
                  </option>
                ))}
              </select>
            ) : (
              <Input
                className="w-56"
                aria-label={t("ai.model")}
                placeholder="gpt-4o-mini"
                value={modelValue}
                onChange={setModel}
                onBlur={() => commitModel(modelValue)}
              />
            )}
            <Button
              variant="secondary"
              size="sm"
              disabled={a.busy === "models" || (status.needs_key && !status.has_key)}
              onClick={() => void a.loadModels()}
            >
              {a.busy === "models" ? t("ai.model.loading") : t("ai.model.load")}
            </Button>
          </Row>

          <Row label={t("ai.test")} hint={t("ai.testHint")}>
            {a.connection === "ok" && (
              <span className="inline-flex items-center gap-1.5 text-xs font-semibold text-success">
                <Check className="size-3.5" />
                {t("ai.test.ok")}
              </span>
            )}
            <Button
              variant="secondary"
              size="sm"
              disabled={a.busy === "test" || (status.needs_key && !status.has_key)}
              onClick={() => void a.test()}
            >
              {a.busy === "test" ? t("ai.test.running") : t("ai.test.run")}
            </Button>
          </Row>
        </>
      )}

      {a.error && <ErrorNote className="my-3">{a.error}</ErrorNote>}
    </Section>
  );
}

/** The state of the chosen model on this machine: downloaded, downloading or to download. */
function LocalModelCard({
  model,
  progress,
  engineFound,
  error,
  onDownload,
  onCancel,
}: {
  model: { id: string; label: string; size: number; license: string; installed: boolean } | undefined;
  progress: { id: string; downloaded: number; total: number } | null;
  engineFound: boolean;
  error: string | null;
  onDownload: (id: string, size: number) => void;
  onCancel: () => void;
}) {
  const t = useT();
  if (!model) return null;
  const downloading = progress?.id === model.id;
  return (
    <div className="my-2 flex flex-col gap-2 rounded-xl border border-border bg-muted/40 p-3.5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <p className="text-sm text-foreground">
          <strong>{model.label}</strong>
          <span className="text-muted-foreground">
            {t("ai.card.meta", { size: gigabytes(model.size), license: model.license })}
          </span>
        </p>
        {downloading ? (
          <div className="flex w-56 flex-col gap-2">
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
        ) : model.installed ? (
          <span className="inline-flex items-center gap-1.5 rounded-full bg-success/10 px-2.5 py-1 text-xs font-semibold text-success">
            <Check className="size-3.5" />
            {t("common.downloaded")}
          </span>
        ) : (
          <Button size="sm" disabled={progress !== null} onClick={() => onDownload(model.id, model.size)}>
            <Download className="size-4" />
            {t("common.download")}
          </Button>
        )}
      </div>
      {!engineFound && (
        <ErrorNote>{t("ai.card.noEngine")}</ErrorNote>
      )}
      {error && <ErrorNote>{error}</ErrorNote>}
    </div>
  );
}
