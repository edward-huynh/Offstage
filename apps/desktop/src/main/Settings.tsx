import { useEffect, useState } from "react";
import {
  Check,
  Download,
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
import { useModel } from "../useModel";
import { useSettings } from "../useSettings";
import { AssistantSettings } from "./AssistantSettings";
import { ErrorNote, Progress, Row, Screen, Section, SegmentedTabs } from "./ui";

const megabytes = (bytes: number) => Math.round(bytes / 1_000_000);

const LANGUAGES: { value: SpeechLanguage; label: string }[] = [
  { value: "auto", label: "Tự nhận" },
  { value: "vietnamese", label: "Tiếng Việt" },
  { value: "english", label: "English" },
];

const HOTKEY_LABELS: Record<HotkeyBinding["action"], string> = {
  play_pause: "Chạy / tạm dừng cuộn",
  speed_up: "Cuộn nhanh hơn",
  speed_down: "Cuộn chậm hơn",
  toggle_visibility: "Ẩn / hiện overlay",
  toggle_click_through: "Chuột xuyên qua",
};

const PROTECTION_TEXT: Record<CaptureProtection, string> = {
  full: "Hệ điều hành này ẩn cửa sổ khỏi chia sẻ màn hình.",
  black_box:
    "Windows bản cũ: người xem sẽ thấy ô đen thay cho cửa sổ. Cần Windows 10 bản 2004 trở lên.",
  best_effort:
    "Có hiệu lực với hầu hết ứng dụng chia sẻ. Một số bản macOS mới có thể vẫn thấy cửa sổ.",
  unknown: "Chưa rõ hệ điều hành này có ẩn được cửa sổ hay không.",
};

/** Sections are read in tabs, one group of related settings at a time. */
const GROUPS = [
  { id: "look", label: "Giao diện", icon: <SlidersHorizontal className="size-4" /> },
  { id: "speech", label: "Giọng nói", icon: <Mic className="size-4" /> },
  { id: "assistant", label: "Trợ lý AI", icon: <Sparkles className="size-4" /> },
  { id: "system", label: "Hệ thống", icon: <ShieldCheck className="size-4" /> },
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
            Đang tải cài đặt…
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
          items={GROUPS}
          value={group}
          onChange={setGroup}
          label="Nhóm cài đặt"
          idPrefix="settings"
        />
      </div>

      <ScrollArea className="min-h-0 flex-1">
        <div className="mx-auto flex w-full flex-col gap-4 px-8 pb-8">
          {error && <ErrorNote>{error}</ErrorNote>}

          <GroupPanel id="look" active={group === "look"}>
            <Section
              title="Overlay"
              hint="Cách teleprompter hiện khi chạy. Đang đọc cũng chỉnh được bằng phím tắt."
              icon={<SlidersHorizontal className="size-4.5" />}
            >
              <SliderRow
                label="Cỡ chữ"
                value={overlay.font_size}
                min={16}
                max={96}
                step={2}
                format={(v) => `${v}px`}
                onChange={(v) => look({ font_size: v })}
              />
              <SliderRow
                label="Tốc độ cuộn"
                value={overlay.scroll_speed}
                min={5}
                max={300}
                step={5}
                format={(v) => `${v} px/s`}
                onChange={(v) => look({ scroll_speed: v })}
              />
              <SliderRow
                label="Độ trong suốt"
                hint="Càng thấp càng thấy rõ phía sau."
                value={overlay.opacity}
                min={0.2}
                max={1}
                step={0.05}
                format={(v) => `${Math.round(v * 100)}%`}
                onChange={(v) => look({ opacity: v })}
              />
              <SliderRow
                label="Vị trí dòng đọc"
                hint="Tính từ mép trên của overlay."
                value={overlay.reading_line}
                min={0.1}
                max={0.9}
                step={0.02}
                format={(v) => `${Math.round(v * 100)}%`}
                onChange={(v) => look({ reading_line: v })}
              />
              <Row label="Lật gương" hint="Dùng khi đọc qua kính phản chiếu.">
                <Switch
                  checked={overlay.mirror}
                  onCheckedChange={(v) => look({ mirror: v })}
                  ariaLabel="Lật gương"
                />
              </Row>
            </Section>
          </GroupPanel>

          <GroupPanel id="speech" active={group === "speech"}>
            <Section
              title="Nhận dạng giọng nói"
              hint="Chạy trên máy bạn, âm thanh không được lưu và không gửi đi đâu."
              icon={<Mic className="size-4.5" />}
            >
              <Row label="Ngôn ngữ" hint="Áp dụng từ lần bắt đầu nghe tiếp theo.">
                <RadioGroup
                  orientation="horizontal"
                  value={settings.speech_language}
                  onValueChange={(v) => void change({ speech_language: v as SpeechLanguage })}
                  aria-label="Ngôn ngữ nhận dạng"
                >
                  {LANGUAGES.map((l) => (
                    <RadioGroupItem key={l.value} value={l.value} label={l.label} />
                  ))}
                </RadioGroup>
              </Row>
              <Row
                label="Mô hình giọng nói"
                hint={
                  model.info
                    ? `${model.info.spec.file_name}, khoảng ${megabytes(model.info.spec.size)} MB. Giấy phép ${model.info.spec.license}.`
                    : undefined
                }
              >
                {model.info === null ? (
                  <span className="text-sm text-muted-foreground">Không đọc được thông tin mô hình</span>
                ) : model.progress ? (
                  <div className="flex w-64 flex-col gap-2">
                    <Progress value={model.progress.downloaded} max={model.progress.total || undefined} />
                    <div className="flex items-center justify-between gap-3 text-xs text-muted-foreground">
                      <span>
                        {megabytes(model.progress.downloaded)}
                        {model.progress.total > 0 ? ` / ${megabytes(model.progress.total)}` : ""} MB
                      </span>
                      <Button variant="ghost" size="sm" onClick={model.cancel}>
                        Huỷ
                      </Button>
                    </div>
                  </div>
                ) : model.installed ? (
                  <span className="inline-flex items-center gap-1.5 rounded-full bg-success/10 px-2.5 py-1 text-xs font-semibold text-success">
                    <Check className="size-3.5" />
                    Đã tải
                  </span>
                ) : (
                  <Button size="sm" onClick={() => void model.download()}>
                    <Download className="size-4" />
                    Tải về
                  </Button>
                )}
              </Row>
              {model.error && <ErrorNote className="my-3">{model.error}</ErrorNote>}
            </Section>

            <Section
              title="Âm thanh và quyền"
              hint="Nguồn được nghe khi bạn bấm Play with AI trong Interview."
              icon={<Volume2 className="size-4.5" />}
            >
              <Row label="Âm thanh hệ thống" hint="Giọng của người bên kia cuộc gọi.">
                <Switch
                  label="Nghe âm thanh hệ thống"
                  checked={sources.system}
                  onCheckedChange={(v) => void change({ interview_system_audio: v })}
                />
              </Row>
              <Row label="Micro" hint="Giọng của bạn.">
                <Switch
                  label="Nghe micro"
                  checked={sources.mic}
                  onCheckedChange={(v) => void change({ interview_microphone: v })}
                />
              </Row>
              <Row label="Quyền truy cập" hint="Nếu máy hỏi quyền mà bạn lỡ từ chối, mở trang cài đặt để cấp lại.">
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => void openPrivacySettings("microphone")}
                >
                  Quyền micro
                </Button>
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => void openPrivacySettings("system_audio")}
                >
                  Quyền âm thanh hệ thống
                </Button>
              </Row>
            </Section>
          </GroupPanel>

          <GroupPanel id="assistant" active={group === "assistant"}>
            <AssistantSettings />
          </GroupPanel>

          <GroupPanel id="system" active={group === "system"}>
            <Section title="Quyền riêng tư" icon={<ShieldCheck className="size-4.5" />}>
              <Row
                label="Ẩn cửa sổ chính khi chia sẻ màn hình"
                hint="Cửa sổ này hiện nội dung, CV và mô tả công việc. Overlay luôn được ẩn."
              >
                <Switch
                  label="Ẩn cửa sổ chính khỏi chia sẻ màn hình"
                  checked={settings.hide_main_from_capture}
                  onCheckedChange={(v) => void change({ hide_main_from_capture: v })}
                />
              </Row>
              <p className="py-3.5 text-xs leading-relaxed text-muted-foreground">
                {PROTECTION_TEXT[protection]}
              </p>
            </Section>

            <Section
              title="Phím tắt"
              hint="Hoạt động cả khi Offstage không ở phía trước. Chưa đổi được."
              icon={<Keyboard className="size-4.5" />}
            >
              <ul className="flex flex-col divide-y divide-border">
                {hotkeys.map((h) => (
                  <li key={h.action} className="flex items-center justify-between gap-4 py-3">
                    <span className="text-sm">{HOTKEY_LABELS[h.action]}</span>
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
