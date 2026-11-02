import { useEffect, useState } from "react";
import {
  Check,
  Download,
  Globe,
  Keyboard,
  Loader2,
  Mic,
  ShieldCheck,
  SlidersHorizontal,
  Sparkles,
  Volume2,
} from "lucide-react";
import { Button } from "@/components/motion/button";
import { RadioGroup, RadioGroupItem } from "@/components/motion/radio";
import { RangeSlider } from "@/components/motion/range-slider";
import { Switch } from "@/components/motion/switch";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "@/lib/utils";
import {
  captureProtection,
  hotkeyBindings,
  openPrivacySettings,
  type CaptureProtection,
  type HotkeyBinding,
  type OverlayConfig,
  type SpeechLanguage,
} from "../tauri";
import { setLanguage, useT, type Key, type Lang } from "../i18n";
import { useModel } from "../useModel";
import { useSettings } from "../useSettings";
import { AssistantSettings } from "./AssistantSettings";
import { ErrorNote, Progress, Row, Screen, Section, SegmentedTabs } from "./ui";

const megabytes = (bytes: number) => Math.round(bytes / 1_000_000);

const LANGUAGES: { value: SpeechLanguage; label: Key }[] = [
  { value: "auto", label: "settings.lang.auto" },
  { value: "vietnamese", label: "settings.lang.vi" },
  { value: "english", label: "settings.lang.en" },
];

const HOTKEY_LABELS: Record<HotkeyBinding["action"], Key> = {
  play_pause: "settings.hotkey.play_pause",
  speed_up: "settings.hotkey.speed_up",
  speed_down: "settings.hotkey.speed_down",
  toggle_visibility: "settings.hotkey.toggle_visibility",
  toggle_click_through: "settings.hotkey.toggle_click_through",
};

const PROTECTION_TEXT: Record<CaptureProtection, Key> = {
  full: "settings.protection.full",
  black_box: "settings.protection.black_box",
  best_effort: "settings.protection.best_effort",
  unknown: "settings.protection.unknown",
};

/** Sections are read in tabs, one group of related settings at a time. */
const GROUPS = [
  { id: "look", label: "settings.group.look", icon: <SlidersHorizontal className="size-4" /> },
  { id: "speech", label: "settings.group.speech", icon: <Mic className="size-4" /> },
  { id: "assistant", label: "settings.group.assistant", icon: <Sparkles className="size-4" /> },
  { id: "system", label: "settings.group.system", icon: <ShieldCheck className="size-4" /> },
] as const;

type GroupId = (typeof GROUPS)[number]["id"];

const GROUP_KEY = "offstage.settings.group";

function readGroup(): GroupId {
  try {
    const saved = window.localStorage.getItem(GROUP_KEY);
    return GROUPS.find((group) => group.id === saved)?.id ?? "look";
  } catch {
    // Storage can be blocked, and the first group then simply opens.
    return "look";
  }
}

/** The open settings tab, remembered between visits when storage allows. */
function useSettingsGroup() {
  const [group, setGroup] = useState<GroupId>(readGroup);

  const select = (next: GroupId) => {
    setGroup(next);
    try {
      window.localStorage.setItem(GROUP_KEY, next);
    } catch {
      // Not remembering the tab is harmless.
    }
  };

  return [group, select] as const;
}

/** One tab's sections. Inactive panels stay mounted, so their state and data survive a switch. */
function GroupPanel({
  id,
  active,
  children,
}: {
  id: GroupId;
  active: boolean;
  children: React.ReactNode;
}) {
  return (
    <div
      role="tabpanel"
      id={`settings-panel-${id}`}
      aria-labelledby={`settings-tab-${id}`}
      className={cn("flex-col gap-4", active ? "flex" : "hidden")}
    >
      {children}
    </div>
  );
}

/** Key combination as shown to the user: ⌘ ⌥ on macOS, Ctrl Alt elsewhere. */
function formatAccelerator(accelerator: string): string {
  const mac = document.documentElement.dataset.platform === "mac";
  const symbols: Record<string, string> = mac
    ? { CommandOrControl: "⌘", Alt: "⌥", Shift: "⇧", Up: "↑", Down: "↓", Left: "←", Right: "→" }
    : { CommandOrControl: "Ctrl", Up: "↑", Down: "↓", Left: "←", Right: "→" };
  return accelerator
    .split("+")
    .map((key) => symbols[key] ?? key)
    .join(mac ? " " : " + ");
}

function SliderRow(props: {
  label: string;
  hint?: string;
  value: number;
  min: number;
  max: number;
  step: number;
  format: (value: number) => string;
  onChange: (value: number) => void;
}) {
  return (
    <Row label={props.label} hint={props.hint}>
      <div className="flex w-64 items-center gap-3">
        <RangeSlider
          className="min-w-0 flex-1"
          value={props.value}
          min={props.min}
          max={props.max}
          step={props.step}
          aria-label={props.label}
          formatValueText={props.format}
          onValueChange={props.onChange}
        />
        <span className="w-16 shrink-0 text-right text-xs tabular-nums text-muted-foreground">
          {props.format(props.value)}
        </span>
      </div>
    </Row>
  );
}

/**
 * Teleprompter look, speech, audio sources, privacy and the shortcut list, in tabs so
 * only one group of sections is on screen at a time.
 */
export function Settings() {
  const t = useT();
  const { settings, overlay, error, change, changeOverlay } = useSettings();
  const model = useModel();
  const [group, setGroup] = useSettingsGroup();
  const [protection, setProtection] = useState<CaptureProtection>("unknown");
  const [hotkeys, setHotkeys] = useState<HotkeyBinding[]>([]);

  useEffect(() => {
    void captureProtection().then(setProtection);
    void hotkeyBindings().then(setHotkeys);
  }, []);

  if (!settings || !overlay) {
    return (
      <Screen scroll={false}>
        {error ? (
          <ErrorNote>{error}</ErrorNote>
        ) : (
          <p className="flex items-center gap-2 text-sm text-muted-foreground">
            <Loader2 className="size-4 animate-spin" />
            {t("settings.loading")}
          </p>
        )}
      </Screen>
    );
  }

  const look = (patch: Partial<OverlayConfig>) => changeOverlay(patch);
  const sources = { system: settings.interview_system_audio, mic: settings.interview_microphone };

  return (
    <section className="flex min-h-0 w-full flex-1 flex-col">
      <div className="shrink-0 px-8 pt-5 pb-4">
        <SegmentedTabs
          items={GROUPS.map((g) => ({ ...g, label: t(g.label) }))}
          value={group}
          onChange={setGroup}
          label={t("settings.groups")}
          idPrefix="settings"
        />
      </div>

      <ScrollArea className="min-h-0 flex-1">
        <div className="mx-auto flex w-full flex-col gap-4 px-8 pb-8">
          {error && <ErrorNote>{error}</ErrorNote>}

          <GroupPanel id="look" active={group === "look"}>
            <Section
              title={t("settings.app.title")}
              hint={t("settings.app.hint")}
              icon={<Globe className="size-4.5" />}
            >
              <Row label={t("settings.app.language")}>
                <RadioGroup
                  orientation="horizontal"
                  value={settings.ui_language}
                  onValueChange={(v) => {
                    // At once, in this window; Rust tells the others.
                    setLanguage(v as Lang);
                    void change({ ui_language: v as Lang });
                  }}
                  aria-label={t("settings.app.language")}
                >
                  <RadioGroupItem value="vi" label={t("settings.lang.vi")} />
                  <RadioGroupItem value="en" label={t("settings.lang.en")} />
                </RadioGroup>
              </Row>
            </Section>

            <Section
              title={t("settings.overlay.title")}
              hint={t("settings.overlay.hint")}
              icon={<SlidersHorizontal className="size-4.5" />}
            >
              <SliderRow
                label={t("settings.overlay.fontSize")}
                value={overlay.font_size}
                min={16}
                max={96}
                step={2}
                format={(v) => `${v}px`}
                onChange={(v) => look({ font_size: v })}
              />
              <SliderRow
                label={t("settings.overlay.speed")}
                value={overlay.scroll_speed}
                min={5}
                max={300}
                step={5}
                format={(v) => `${v} px/s`}
                onChange={(v) => look({ scroll_speed: v })}
              />
              <SliderRow
                label={t("settings.overlay.opacity")}
                hint={t("settings.overlay.opacityHint")}
                value={overlay.opacity}
                min={0.2}
                max={1}
                step={0.05}
                format={(v) => `${Math.round(v * 100)}%`}
                onChange={(v) => look({ opacity: v })}
              />
              <SliderRow
                label={t("settings.overlay.readingLine")}
                hint={t("settings.overlay.readingLineHint")}
                value={overlay.reading_line}
                min={0.1}
                max={0.9}
                step={0.02}
                format={(v) => `${Math.round(v * 100)}%`}
                onChange={(v) => look({ reading_line: v })}
              />
              <Row label={t("settings.overlay.mirror")} hint={t("settings.overlay.mirrorHint")}>
                <Switch
                  checked={overlay.mirror}
                  onCheckedChange={(v) => look({ mirror: v })}
                  ariaLabel={t("settings.overlay.mirror")}
                />
              </Row>
            </Section>
          </GroupPanel>

          <GroupPanel id="speech" active={group === "speech"}>
            <Section
              title={t("settings.speech.title")}
              hint={t("settings.speech.hint")}
              icon={<Mic className="size-4.5" />}
            >
              <Row label={t("settings.speech.language")} hint={t("settings.speech.languageHint")}>
                <RadioGroup
                  orientation="horizontal"
                  value={settings.speech_language}
                  onValueChange={(v) => void change({ speech_language: v as SpeechLanguage })}
                  aria-label={t("settings.speech.languageLabel")}
                >
                  {LANGUAGES.map((l) => (
                    <RadioGroupItem key={l.value} value={l.value} label={t(l.label)} />
                  ))}
                </RadioGroup>
              </Row>
              <Row
                label={t("settings.speech.model")}
                hint={
                  model.info
                    ? t("settings.speech.modelHint", {
                        file: model.info.spec.file_name,
                        size: megabytes(model.info.spec.size),
                        license: model.info.spec.license,
                      })
                    : undefined
                }
              >
                {model.info === null ? (
                  <span className="text-sm text-muted-foreground">{t("settings.speech.modelUnknown")}</span>
                ) : model.progress ? (
                  <div className="flex w-64 flex-col gap-2">
                    <Progress value={model.progress.downloaded} max={model.progress.total || undefined} />
                    <div className="flex items-center justify-between gap-3 text-xs text-muted-foreground">
                      <span>
                        {megabytes(model.progress.downloaded)}
                        {model.progress.total > 0 ? ` / ${megabytes(model.progress.total)}` : ""} MB
                      </span>
                      <Button variant="ghost" size="sm" onClick={model.cancel}>
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
                  <Button size="sm" onClick={() => void model.download()}>
                    <Download className="size-4" />
                    {t("common.download")}
                  </Button>
                )}
              </Row>
              {model.error && <ErrorNote className="my-3">{model.error}</ErrorNote>}
            </Section>

            <Section
              title={t("settings.audio.title")}
              hint={t("settings.audio.hint")}
              icon={<Volume2 className="size-4.5" />}
            >
              <Row label={t("settings.audio.system")} hint={t("settings.audio.systemHint")}>
                <Switch
                  label={t("settings.audio.systemLabel")}
                  checked={sources.system}
                  onCheckedChange={(v) => void change({ interview_system_audio: v })}
                />
              </Row>
              <Row label={t("settings.audio.mic")} hint={t("settings.audio.micHint")}>
                <Switch
                  label={t("settings.audio.micLabel")}
                  checked={sources.mic}
                  onCheckedChange={(v) => void change({ interview_microphone: v })}
                />
              </Row>
              <Row label={t("settings.audio.access")} hint={t("settings.audio.accessHint")}>
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => void openPrivacySettings("microphone")}
                >
                  {t("settings.audio.micPermission")}
                </Button>
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => void openPrivacySettings("system_audio")}
                >
                  {t("settings.audio.systemPermission")}
                </Button>
              </Row>
            </Section>
          </GroupPanel>

          <GroupPanel id="assistant" active={group === "assistant"}>
            <AssistantSettings />
          </GroupPanel>

          <GroupPanel id="system" active={group === "system"}>
            <Section title={t("settings.privacy.title")} icon={<ShieldCheck className="size-4.5" />}>
              <Row
                label={t("settings.privacy.hideMain")}
                hint={t("settings.privacy.hideMainHint")}
              >
                <Switch
                  label={t("settings.privacy.hideMainLabel")}
                  checked={settings.hide_main_from_capture}
                  onCheckedChange={(v) => void change({ hide_main_from_capture: v })}
                />
              </Row>
              <p className="py-3.5 text-xs leading-relaxed text-muted-foreground">
                {t(PROTECTION_TEXT[protection])}
              </p>
            </Section>

            <Section
              title={t("settings.hotkeys.title")}
              hint={t("settings.hotkeys.hint")}
              icon={<Keyboard className="size-4.5" />}
            >
              <ul className="flex flex-col divide-y divide-border">
                {hotkeys.map((h) => (
                  <li key={h.action} className="flex items-center justify-between gap-4 py-3">
                    <span className="text-sm">{t(HOTKEY_LABELS[h.action])}</span>
                    <kbd
                      className={cn(
                        "shrink-0 rounded-md border border-border bg-background px-2 py-1 font-sans text-[11px] text-foreground",
                      )}
                    >
                      {formatAccelerator(h.accelerator)}
                    </kbd>
                  </li>
                ))}
              </ul>
            </Section>
          </GroupPanel>
        </div>
      </ScrollArea>
    </section>
  );
}
