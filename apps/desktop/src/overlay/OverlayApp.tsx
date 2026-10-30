import { useEffect, useMemo, useRef, useState } from "react";
import { TranscriptPanel } from "../TranscriptPanel";
import {
  captureProtection,
  getSettings,
  hotkeyFailures,
  loadConfig,
  onClickThrough,
  onHotkey,
  saveConfig,
  setClickThrough,
  stopPlay,
  type CaptureProtection,
  type OverlayConfig,
  type Speaker,
} from "../tauri";
import { useAutoScroll } from "../useAutoScroll";
import { useListening } from "../useListening";
import { usePlay } from "../usePlay";

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
// Keeps repeated 0.05 steps from drifting into values like 0.7500000000000001.
const round2 = (v: number) => Math.round(v * 100) / 100;

/** The overlay: reads the content being played, or shows live captions in an interview. */
export function OverlayApp() {
  const [cfg, setCfg] = useState<OverlayConfig | null>(null);
  const [playing, setPlaying] = useState(false);
  const [captionsOpen, setCaptionsOpen] = useState(false);
  const play = usePlay();
  const interview = play.session?.mode === "interview";
  const listening = useListening(captionsOpen);
  const toggleSource = listening.toggle;
  const startSources = listening.startAll;
  const isListening = listening.active.me || listening.active.remote;
  const [scroller, setScroller] = useState<HTMLDivElement | null>(null);

  // The overlay stays alive while hidden, and Settings changes the saved config meanwhile,
  // so each new session reads it again. Without this the next save would bring old values back.
  useEffect(() => {
    loadConfig().then(setCfg);
  }, [play.session]);

  // Persist changes after a short pause so holding a key does not write on every repeat.
  // The first value is the one just loaded, so it is skipped.
  const loadedCfg = useRef<OverlayConfig | null>(null);
  useEffect(() => {
    if (!cfg) return;
    if (loadedCfg.current === null) {
      loadedCfg.current = cfg;
      return;
    }
    const id = window.setTimeout(() => void saveConfig(cfg), 300);
    return () => window.clearTimeout(id);
  }, [cfg]);

  useAutoScroll(scroller, playing, cfg?.scroll_speed ?? 0);

  // The overlay re-renders on every caption line and model-download tick, so the paragraph
  // split (and its regex over the whole script) is kept until the script itself changes.
  const paragraphs = useMemo(() => play.body.split(/\n{2,}/), [play.body]);

  // A new session starts paused, with the captions only in an interview.
  useEffect(() => {
    setPlaying(false);
    setCaptionsOpen(interview);
  }, [play.session, interview]);

  // An interview starts listening to the sources chosen in Settings, so the captions are
  // running by the time the call begins.
  useEffect(() => {
    if (!interview) return;
    let cancelled = false;
    void getSettings().then((s) => {
      if (cancelled) return;
      const speakers: Speaker[] = [];
      if (s.interview_system_audio) speakers.push("remote");
      if (s.interview_microphone) speakers.push("me");
      void startSources(speakers);
    });
    return () => {
      cancelled = true;
    };
  }, [play.session, interview, startSources]);

  // Back to the top for each new content.
  useEffect(() => {
    scroller?.scrollTo({ top: 0 });
  }, [play.body, scroller]);

  const [hotkeyIssues, setHotkeyIssues] = useState<string[]>([]);
  useEffect(() => {
    hotkeyFailures().then(setHotkeyIssues);
  }, []);

  // Split-screen protection is the product's promise, so the overlay says how well it holds.
  // "unknown" stays quiet: it means the platform was not probed, not that sharing leaks.
  const [protection, setProtection] = useState<CaptureProtection>("unknown");
  useEffect(() => {
    captureProtection().then(setProtection);
  }, []);

  // Global hotkeys arrive even when the overlay is not focused. They use functional
  // updates so the listener is registered once instead of on every change.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    onHotkey((action) => {
      switch (action) {
        case "play_pause":
          setPlaying((p) => !p);
          break;
        case "speed_up":
          setCfg((c) => c && { ...c, scroll_speed: clamp(c.scroll_speed + 5, 5, 300) });
          break;
        case "speed_down":
          setCfg((c) => c && { ...c, scroll_speed: clamp(c.scroll_speed - 5, 5, 300) });
          break;
      }
    }).then((off) => {
      if (cancelled) off();
      else unlisten = off;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  // Rust owns click-through; the UI only mirrors it for the indicator.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    onClickThrough((enabled) => {
      setCfg((c) => c && { ...c, click_through: enabled });
    }).then((off) => {
      if (cancelled) off();
      else unlisten = off;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  // Holding a key changes the config on every repeat, so the handler must not depend on it:
  // re-registering the listener dozens of times a second is pure overhead. Everything it
  // reads lives in refs, and config changes go through functional updates.
  const live = useRef({ cfg, captionsOpen, scroller, toggleSource });
  useEffect(() => {
    live.current = { cfg, captionsOpen, scroller, toggleSource };
  });

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const { cfg, captionsOpen, scroller, toggleSource } = live.current;
      if (!cfg) return;
      const set = (patch: Partial<OverlayConfig>) =>
        setCfg((c) => (c ? { ...c, ...patch } : c));
      switch (e.key) {
        case " ":
          e.preventDefault();
          setPlaying((p) => !p);
          break;
        case "ArrowUp":
          set({ scroll_speed: clamp(cfg.scroll_speed + 5, 5, 300) });
          break;
        case "ArrowDown":
          set({ scroll_speed: clamp(cfg.scroll_speed - 5, 5, 300) });
          break;
        case "+":
        case "=":
          set({ font_size: clamp(cfg.font_size + 2, 16, 96) });
          break;
        case "-":
          set({ font_size: clamp(cfg.font_size - 2, 16, 96) });
          break;
        case "[":
          set({ opacity: clamp(round2(cfg.opacity - 0.05), 0.2, 1) });
          break;
        case "]":
          set({ opacity: clamp(round2(cfg.opacity + 0.05), 0.2, 1) });
          break;
        case ",":
          set({ reading_line: clamp(round2(cfg.reading_line - 0.02), 0.1, 0.9) });
          break;
        case ".":
          set({ reading_line: clamp(round2(cfg.reading_line + 0.02), 0.1, 0.9) });
          break;
        case "m":
          set({ mirror: !cfg.mirror });
          break;
        case "c":
          void setClickThrough(true);
          break;
        case "t":
          setCaptionsOpen((open) => !open);
          break;
        case "1":
          if (captionsOpen) void toggleSource("me");
          break;
        case "2":
          if (captionsOpen) void toggleSource("remote");
          break;
        case "Home":
          scroller?.scrollTo({ top: 0 });
          break;
        case "Escape":
          void stopPlay();
          break;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // Nothing to show until the overlay has its settings and a session to play.
  if (!cfg || !play.session) return null;

  return (
    <main
      className={cfg.click_through ? "overlay through" : "overlay"}
      style={{ ["--bg-alpha" as string]: cfg.opacity }}
    >
      <header className="bar" data-tauri-drag-region>
        <span className={playing ? "dot live" : "dot"} aria-hidden />
        {interview && <span data-tauri-drag-region>Interview</span>}
        {play.title && (
          <span className="script-title" title={play.title} data-tauri-drag-region>
            {play.title}
          </span>
        )}
        <span className="stop-hint" data-tauri-drag-region>
          Esc để dừng
        </span>
        <span className="spacer" data-tauri-drag-region />
        {isListening && (
          <span className="warn" title="Mic hoặc âm thanh hệ thống đang được nhận dạng" data-tauri-drag-region>
            ● Đang nghe
          </span>
        )}
        {cfg.click_through && (
          <span className="warn" data-tauri-drag-region>
            Chuột xuyên qua · Ctrl/⌘+Alt+T để tắt
          </span>
        )}
        {hotkeyIssues.length > 0 && (
          <span className="warn" title={hotkeyIssues.join("\n")}>
            {hotkeyIssues.length} phím tắt toàn cục lỗi
          </span>
        )}
        {protection === "full" && (
          <span
            className="bar-note"
            title="Cửa sổ này không xuất hiện khi bạn chia sẻ màn hình"
            data-tauri-drag-region
          >
            Ẩn khỏi chia sẻ
          </span>
        )}
        {(protection === "best_effort" || protection === "black_box") && (
          <span
            className="warn"
            title="Hệ điều hành có thể vẫn cho người khác thấy cửa sổ này khi bạn chia sẻ màn hình"
            data-tauri-drag-region
          >
            Có thể bị thấy
          </span>
        )}
        <button
          type="button"
          className="bar-btn"
          onClick={() => setPlaying((p) => !p)}
          aria-label={playing ? "Tạm dừng (Space)" : "Chạy (Space)"}
          title={playing ? "Tạm dừng (Space)" : "Chạy (Space)"}
          disabled={cfg.click_through}
        >
          {playing ? "Tạm dừng" : "Chạy"}
          {!playing && <kbd>Space</kbd>}
        </button>
      </header>

      {protection === "black_box" && (
        <p className="notice" role="status">
          Windows bản cũ: khi chia sẻ màn hình, người xem sẽ thấy ô đen thay cho cửa sổ này. Cần Windows 10
          bản 2004 trở lên.
        </p>
      )}

      {play.error && (
        <p className="notice" role="alert">
          {play.error}
        </p>
      )}

      {interview ? (
        <div className="interview-note">
          <strong>Play with AI</strong>
          <p>Phụ đề đang chạy bên dưới. Trợ lý AI sẽ gợi ý câu trả lời ở bản sau, hiện chưa có.</p>
        </div>
      ) : (
        <div
          ref={setScroller}
          className={cfg.mirror ? "script mirror" : "script"}
          style={{ fontSize: cfg.font_size }}
        >
          <div className="pad" style={{ height: `${cfg.reading_line * 100}%` }} />
          {paragraphs.map((p, i) => (
            <p key={i}>{p}</p>
          ))}
          <div className="pad" />
        </div>
      )}
      {captionsOpen && (
        <TranscriptPanel
          lines={listening.lines}
          active={listening.active}
          busy={listening.busy}
          error={listening.error}
          model={listening.model}
          needsModel={listening.needsModel}
          progress={listening.progress}
          latencyMs={listening.latencyMs}
          permissionIssue={listening.permissionIssue}
          onToggle={toggleSource}
          onOpenSettings={listening.openSettings}
          onRetryPermission={listening.retryPermission}
          onDownload={listening.startDownload}
          onCancel={listening.cancel}
          onClear={listening.clear}
          onDismissError={listening.dismissError}
        />
      )}
      {!interview && <div className="cue-line" style={{ top: `${cfg.reading_line * 100}%` }} aria-hidden />}
    </main>
  );
}
