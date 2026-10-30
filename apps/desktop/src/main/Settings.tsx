import { useEffect, useState, type ReactNode } from "react";
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
  black_box: "Windows bản cũ: người xem sẽ thấy ô đen thay cho cửa sổ. Cần Windows 10 bản 2004 trở lên.",
  best_effort: "Có hiệu lực với hầu hết ứng dụng chia sẻ. Một số bản macOS mới có thể vẫn thấy cửa sổ.",
  unknown: "Chưa rõ hệ điều hành này có ẩn được cửa sổ hay không.",
};

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

function Section({ title, hint, children }: { title: string; hint?: string; children: ReactNode }) {
  return (
    <section className="mn-set-section">
      <h2>{title}</h2>
      {hint && <p className="mn-set-hint">{hint}</p>}
      <div className="mn-set-rows">{children}</div>
    </section>
  );
}

function Row({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <div className="mn-set-row">
      <div className="mn-set-label">
        <span>{label}</span>
        {hint && <small>{hint}</small>}
      </div>
      <div className="mn-set-control">{children}</div>
    </div>
  );
}

function Switch({ label, checked, onChange }: { label: string; checked: boolean; onChange: (value: boolean) => void }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      className={checked ? "mn-switch is-on" : "mn-switch"}
      onClick={() => onChange(!checked)}
    >
      <span />
    </button>
  );
}

function Slider(props: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  format: (value: number) => string;
  onChange: (value: number) => void;
}) {
  return (
    <>
      <input
        type="range"
        className="mn-range"
        aria-label={props.label}
        min={props.min}
        max={props.max}
        step={props.step}
        value={props.value}
        onChange={(e) => props.onChange(Number(e.target.value))}
      />
      <output className="mn-range-value">{props.format(props.value)}</output>
    </>
  );
}

/** Teleprompter look, speech, audio sources, privacy and the shortcut list. */
export function Settings() {
  const { settings, overlay, error, change, changeOverlay } = useSettings();
  const model = useModel();
  const [protection, setProtection] = useState<CaptureProtection>("unknown");
  const [hotkeys, setHotkeys] = useState<HotkeyBinding[]>([]);

  useEffect(() => {
    void captureProtection().then(setProtection);
    void hotkeyBindings().then(setHotkeys);
  }, []);

  if (!settings || !overlay) {
    return (
      <section className="mn-screen">
        {error ? (
          <p className="mn-error" role="alert">
            {error}
          </p>
        ) : (
          <p className="mn-lead">Đang tải cài đặt…</p>
        )}
      </section>
    );
  }

  const look = (patch: Partial<OverlayConfig>) => changeOverlay(patch);
  const sources = { system: settings.interview_system_audio, mic: settings.interview_microphone };

  return (
    <section className="mn-screen">
      {error && (
        <p className="mn-error" role="alert">
          {error}
        </p>
      )}

      <Section title="Overlay" hint="Cách teleprompter hiện khi chạy. Đang đọc cũng chỉnh được bằng phím tắt.">
        <Row label="Cỡ chữ">
          <Slider label="Cỡ chữ" value={overlay.font_size} min={16} max={96} step={2} format={(v) => `${v}px`} onChange={(v) => look({ font_size: v })} />
        </Row>
        <Row label="Tốc độ cuộn">
          <Slider label="Tốc độ cuộn" value={overlay.scroll_speed} min={5} max={300} step={5} format={(v) => `${v} px/s`} onChange={(v) => look({ scroll_speed: v })} />
        </Row>
        <Row label="Độ trong suốt" hint="Càng thấp càng thấy rõ phía sau.">
          <Slider label="Độ đục của nền" value={overlay.opacity} min={0.2} max={1} step={0.05} format={(v) => `${Math.round(v * 100)}%`} onChange={(v) => look({ opacity: v })} />
        </Row>
        <Row label="Vị trí dòng đọc" hint="Tính từ mép trên của overlay.">
          <Slider label="Vị trí dòng đọc" value={overlay.reading_line} min={0.1} max={0.9} step={0.02} format={(v) => `${Math.round(v * 100)}%`} onChange={(v) => look({ reading_line: v })} />
        </Row>
        <Row label="Lật gương" hint="Dùng khi đọc qua kính phản chiếu.">
          <Switch label="Lật gương" checked={overlay.mirror} onChange={(v) => look({ mirror: v })} />
        </Row>
      </Section>

      <Section title="Nhận dạng giọng nói" hint="Chạy trên máy bạn, âm thanh không được lưu và không gửi đi đâu.">
        <Row label="Ngôn ngữ" hint="Áp dụng từ lần bắt đầu nghe tiếp theo.">
          <div className="mn-seg" role="radiogroup" aria-label="Ngôn ngữ nhận dạng">
            {LANGUAGES.map((l) => (
              <button
                key={l.value}
                type="button"
                role="radio"
                aria-checked={settings.speech_language === l.value}
                className={settings.speech_language === l.value ? "mn-seg-item is-selected" : "mn-seg-item"}
                onClick={() => void change({ speech_language: l.value })}
              >
                {l.label}
              </button>
            ))}
          </div>
        </Row>
        <Row
          label="Mô hình giọng nói"
          hint={model.info ? `${model.info.spec.file_name}, khoảng ${megabytes(model.info.spec.size)} MB. Giấy phép ${model.info.spec.license}.` : undefined}
        >
          {model.info === null ? (
            <span className="mn-set-status">Không đọc được thông tin mô hình</span>
          ) : model.progress ? (
            <>
              <progress className="mn-progress" value={model.progress.downloaded} max={model.progress.total || undefined} />
              <button type="button" className="mn-btn secondary" onClick={model.cancel}>
                Huỷ
              </button>
            </>
          ) : model.installed ? (
            <span className="mn-set-status ok">Đã tải</span>
          ) : (
            <button type="button" className="mn-btn primary" onClick={() => void model.download()}>
              Tải về
            </button>
          )}
        </Row>
        {model.error && (
          <p className="mn-error" role="alert">
            {model.error}
          </p>
        )}
      </Section>

      <Section title="Âm thanh và quyền" hint="Nguồn được nghe khi bạn bấm Play with AI trong Interview.">
        <Row label="Âm thanh hệ thống" hint="Giọng của người bên kia cuộc gọi.">
          <Switch
            label="Nghe âm thanh hệ thống"
            checked={sources.system}
            onChange={(v) => void change({ interview_system_audio: v })}
          />
        </Row>
        <Row label="Micro" hint="Giọng của bạn.">
          <Switch label="Nghe micro" checked={sources.mic} onChange={(v) => void change({ interview_microphone: v })} />
        </Row>
        <Row label="Quyền truy cập" hint="Nếu máy hỏi quyền mà bạn lỡ từ chối, mở trang cài đặt để cấp lại.">
          <button type="button" className="mn-btn secondary" onClick={() => void openPrivacySettings("microphone")}>
            Quyền micro
          </button>
          <button type="button" className="mn-btn secondary" onClick={() => void openPrivacySettings("system_audio")}>
            Quyền âm thanh hệ thống
          </button>
        </Row>
      </Section>

      <Section title="Quyền riêng tư">
        <Row label="Ẩn cửa sổ chính khi chia sẻ màn hình" hint="Cửa sổ này hiện nội dung, CV và mô tả công việc. Overlay luôn được ẩn.">
          <Switch
            label="Ẩn cửa sổ chính khỏi chia sẻ màn hình"
            checked={settings.hide_main_from_capture}
            onChange={(v) => void change({ hide_main_from_capture: v })}
          />
        </Row>
        <p className="mn-set-note">{PROTECTION_TEXT[protection]}</p>
      </Section>

      <Section title="Phím tắt" hint="Hoạt động cả khi Offstage không ở phía trước. Chưa đổi được.">
        <ul className="mn-keys">
          {hotkeys.map((h) => (
            <li key={h.action}>
              <span>{HOTKEY_LABELS[h.action]}</span>
              <kbd>{formatAccelerator(h.accelerator)}</kbd>
            </li>
          ))}
        </ul>
      </Section>
    </section>
  );
}
