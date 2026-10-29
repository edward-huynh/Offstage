import { useCallback, useEffect, useRef, useState } from "react";
import { ScriptPanel } from "./ScriptPanel";
import { TranscriptPanel } from "./TranscriptPanel";
import {
  captureProtection,
  hideOverlay,
  hotkeyFailures,
  loadConfig,
  onClickThrough,
  onHotkey,
  saveConfig,
  setClickThrough,
  type OverlayConfig,
} from "./tauri";
import { useAutoScroll } from "./useAutoScroll";
import { useListening } from "./useListening";
import { useScripts } from "./useScripts";

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
// Keeps repeated 0.05 steps from drifting into values like 0.7500000000000001.
const round2 = (v: number) => Math.round(v * 100) / 100;

export function App() {
  const [cfg, setCfg] = useState<OverlayConfig | null>(null);
  const [playing, setPlaying] = useState(false);
  const [editing, setEditing] = useState(false);
  const [panelOpen, setPanelOpen] = useState(false);
  const [captionsOpen, setCaptionsOpen] = useState(false);
  const listening = useListening(captionsOpen);
  const toggleSource = listening.toggle;
  const isListening = listening.active.me || listening.active.remote;
  const [scroller, setScroller] = useState<HTMLDivElement | null>(null);

  const setActiveScript = useCallback((id: string) => {
    setCfg((c) => (c && c.active_script !== id ? { ...c, active_script: id } : c));
  }, []);
  const scripts = useScripts(cfg !== null, cfg?.active_script ?? null, setActiveScript);
  const { flush, refresh, clearError } = scripts;

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

  useAutoScroll(scroller, playing && !editing, cfg?.scroll_speed ?? 0);

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
  // updates and a ref so the listener is registered once instead of on every change.
  const typingRef = useRef(false);
  useEffect(() => {
    typingRef.current = editing || panelOpen;
  }, [editing, panelOpen]);
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    onHotkey((action) => {
      if (typingRef.current) return;
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
      if (editing) {
        if (e.key === "Escape") {
          setEditing(false);
          void flush();
        }
        return;
      }
      if (panelOpen) {
        // Typing in the rename box is handled by the box itself.
        if ((e.target as HTMLElement).tagName === "INPUT") return;
        if (e.key === "Escape") setPanelOpen(false);
        return;
      }
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
        case "e":
          setPlaying(false);
          setEditing(true);
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
        case "l":
          setPlaying(false);
          clearError();
          void refresh();
          setPanelOpen(true);
          break;
        case "Home":
          scroller?.scrollTo({ top: 0 });
          break;
        case "Escape":
          void hideOverlay();
          break;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [cfg, editing, panelOpen, captionsOpen, toggleSource, scroller, flush, refresh, clearError]);

  if (!cfg) return null;

  const activeTitle = scripts.scripts.find((s) => s.id === cfg.active_script)?.title;

  return (
    <main
      className={cfg.click_through ? "overlay through" : "overlay"}
      style={{ ["--bg-alpha" as string]: cfg.opacity }}
    >
      <header className="bar" data-tauri-drag-region>
        <span className={playing ? "dot live" : "dot"} aria-hidden />
        <span data-tauri-drag-region>
          {editing ? "Đang sửa (Esc để xong)" : panelOpen ? "Chọn kịch bản" : playing ? "Đang chạy" : "Tạm dừng"}
        </span>
        {activeTitle && !panelOpen && (
          <span className="script-title" title={activeTitle} data-tauri-drag-region>
            {activeTitle}
          </span>
        )}
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

      {panelOpen ? (
        <ScriptPanel
          scripts={scripts.scripts}
          activeId={cfg.active_script}
          error={scripts.error}
          onClose={() => setPanelOpen(false)}
          onSelect={async (id) => {
            if (await scripts.select(id)) setPanelOpen(false);
          }}
          onCreate={async () => {
            if (await scripts.create()) {
              setPanelOpen(false);
              setEditing(true);
            }
          }}
          onImport={async () => {
            if (await scripts.importFile()) setPanelOpen(false);
          }}
          onRename={(id, title) => void scripts.rename(id, title)}
          onRemove={(id) => void scripts.remove(id)}
        />
      ) : editing ? (
        <textarea
          className="editor"
          value={scripts.body}
          autoFocus
          onChange={(e) => scripts.edit(e.target.value)}
          aria-label="Nội dung kịch bản"
        />
      ) : (
        <div
          ref={setScroller}
          className={cfg.mirror ? "script mirror" : "script"}
          style={{ fontSize: cfg.font_size }}
        >
          <div className="pad" style={{ height: `${cfg.reading_line * 100}%` }} />
          {scripts.body.split(/\n{2,}/).map((p, i) => (
            <p key={i}>{p}</p>
          ))}
          <div className="pad" />
        </div>
      )}
      {captionsOpen && !editing && !panelOpen && (
        <TranscriptPanel
          lines={listening.lines}
          active={listening.active}
          busy={listening.busy}
          error={listening.error}
          model={listening.model}
          needsModel={listening.needsModel}
          progress={listening.progress}
          latencyMs={listening.latencyMs}
          onToggle={(speaker) => void toggleSource(speaker)}
          onDownload={() => void listening.startDownload()}
          onCancel={listening.cancel}
          onClear={listening.clear}
          onDismissError={listening.dismissError}
        />
      )}
      {!editing && !panelOpen && <div className="cue-line" style={{ top: `${cfg.reading_line * 100}%` }} aria-hidden />}
    </main>
  );
}
