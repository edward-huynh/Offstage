import { useEffect, useRef, useState } from "react";
import { TranscriptPanel } from "../TranscriptPanel";
import {
  captureProtection,
  hotkeyFailures,
  loadConfig,
  onClickThrough,
  onHotkey,
  saveConfig,
  setClickThrough,
  stopPlay,
  type OverlayConfig,
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
  const isListening = listening.active.me || listening.active.remote;
  const [scroller, setScroller] = useState<HTMLDivElement | null>(null);

  useEffect(() => {
    loadConfig().then(setCfg);
  }, []);

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

  // A new session starts paused, with the captions only in an interview.
  useEffect(() => {
    setPlaying(false);
    setCaptionsOpen(interview);
  }, [play.session, interview]);

  // Back to the top for each new content.
  useEffect(() => {
    scroller?.scrollTo({ top: 0 });
  }, [play.body, scroller]);

  const [hotkeyIssues, setHotkeyIssues] = useState<string[]>([]);
  useEffect(() => {
    hotkeyFailures().then(setHotkeyIssues);
  }, []);

  // Only a known problem is worth a warning. "unknown" and "best_effort" stay silent.
  const [blackBox, setBlackBox] = useState(false);
  useEffect(() => {
    captureProtection().then((p) => setBlackBox(p === "black_box"));
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

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!cfg) return;
      const set = (patch: Partial<OverlayConfig>) => setCfg({ ...cfg, ...patch });
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
  }, [cfg, captionsOpen, toggleSource, scroller]);

  // Nothing to show until the overlay has its settings and a session to play.
  if (!cfg || !play.session) return null;

  return (
    <main
      className={cfg.click_through ? "overlay through" : "overlay"}
      style={{ ["--bg-alpha" as string]: cfg.opacity }}
    >
      <header className="bar" data-tauri-drag-region>
        <span className={playing ? "dot live" : "dot"} aria-hidden />
        <span data-tauri-drag-region>
          {interview ? "Interview" : playing ? "Đang chạy" : "Tạm dừng"}
        </span>
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
        <span data-tauri-drag-region>{cfg.scroll_speed} px/s</span>
        <span data-tauri-drag-region>{cfg.font_size}px</span>
        <span data-tauri-drag-region>{Math.round(cfg.opacity * 100)}%</span>
      </header>

      {blackBox && (
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
          {play.body.split(/\n{2,}/).map((p, i) => (
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
          onToggle={(speaker) => void toggleSource(speaker)}
          onOpenSettings={() => void listening.openSettings()}
          onRetryPermission={() => void listening.retryPermission()}
          onDownload={() => void listening.startDownload()}
          onCancel={listening.cancel}
          onClear={listening.clear}
          onDismissError={listening.dismissError}
        />
      )}
      {!interview && <div className="cue-line" style={{ top: `${cfg.reading_line * 100}%` }} aria-hidden />}
    </main>
  );
}
