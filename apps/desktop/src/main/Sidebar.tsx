import { motion } from "motion/react";
import {
  BookOpen,
  FileText,
  Files,
  Home,
  PanelLeft,
  Plus,
  Search,
  Settings2,
  Upload,
  type LucideIcon,
} from "lucide-react";
import { Button } from "@/components/motion/button";
import { cn } from "@/lib/utils";
import type { Library } from "../useLibrary";
import type { Screen } from "./MainApp";

type Props = {
  screen: Screen;
  onNavigate: (screen: Screen) => void;
  library: Library;
  collapsed: boolean;
  onToggle: () => void;
  onSearch: () => void;
};

const NAV: Array<{ screen: Screen; label: string; icon: LucideIcon }> = [
  { screen: "home", label: "Home", icon: Home },
  { screen: "content", label: "Content", icon: Files },
  { screen: "knowledge", label: "Knowledge", icon: BookOpen },
];

const ACTIVE_SPRING = { type: "spring", stiffness: 520, damping: 42 } as const;

/**
 * A Dia-style tab rail: the screens and the contents are rows, the active row is a
 * filled pill that glides. Create sits above the tabs, search and import on the
 * Content section, and Settings on its own at the bottom.
 *
 * Collapsed, the rail leaves the layout and floats in when the pointer reaches the
 * left edge (or the handle is clicked), then slides away again on leave.
 */
export function Sidebar(props: Props) {
  if (props.collapsed) return <FloatingSidebar {...props} />;
  return <SidebarPanel {...props} />;
}

function FloatingSidebar(props: Props) {
  return (
    <div className="group fixed inset-y-0 left-0 z-40 w-1.5">
      <button
        type="button"
        onClick={props.onToggle}
        aria-label="Ghim thanh bên"
        title="Ghim thanh bên"
        className={cn(
          "absolute left-0 top-1/2 z-20 flex h-10 w-6 -translate-y-1/2 items-center justify-center",
          "rounded-r-lg border border-l-0 border-border/60 bg-[#0e131a] text-muted-foreground/50",
          "transition-[opacity,color] duration-150 hover:text-foreground group-hover:pointer-events-none group-hover:opacity-0",
        )}
      >
        <PanelLeft className="size-3.5" />
      </button>

      <div
        className={cn(
          "absolute inset-y-0 left-0 z-10 w-[252px] -translate-x-full overflow-hidden rounded-r-xl",
          "border-r border-border/60 bg-[#0e131a] shadow-2xl shadow-black/50",
          "transition-transform duration-200 ease-out group-hover:translate-x-0",
        )}
      >
        <SidebarPanel {...props} floating />
      </div>
    </div>
  );
}

function SidebarPanel({ screen, onNavigate, library, onToggle, onSearch, floating }: Props & {
  floating?: boolean;
}) {
  const create = async () => {
    if (await library.create()) onNavigate("content");
  };
  const importFile = async () => {
    if (await library.importFile()) onNavigate("content");
  };
  const open = (id: string) => {
    onNavigate("content");
    void library.select(id);
  };

  return (
    <nav
      aria-label="Điều hướng"
      className={cn(
        "flex h-full min-h-0 flex-col bg-[#0e131a] px-2",
        !floating && "border-r border-border/60",
      )}
    >
      <div className="mn-topbar flex shrink-0 items-center justify-end" data-tauri-drag-region>
        <Button
          variant="ghost"
          size="icon"
          onClick={onToggle}
          aria-label={floating ? "Ghim thanh bên" : "Thu gọn thanh bên"}
          title={floating ? "Ghim thanh bên" : "Thu gọn thanh bên"}
          className="size-8 text-muted-foreground/70 hover:text-foreground"
        >
          <PanelLeft className="size-4" />
        </Button>
      </div>

      <button
        type="button"
        onClick={() => void create()}
        title="Content mới"
        className="mb-1 flex h-9 w-full shrink-0 items-center gap-2.5 rounded-lg px-3 text-[13px] font-medium text-muted-foreground transition-colors hover:bg-white/[0.04] hover:text-foreground"
      >
        <Plus className="size-4 shrink-0" />
        <span>Content mới</span>
      </button>

      <ul className="flex flex-col gap-0.5">
        {NAV.map((item) => {
          const active = item.screen === screen;
          const Icon = item.icon;
          return (
            <li key={item.screen}>
              <button
                type="button"
                onClick={() => onNavigate(item.screen)}
                aria-current={active ? "page" : undefined}
                title={item.label}
                className={cn(
                  "relative flex h-9 w-full items-center gap-2.5 rounded-lg px-3 text-[13px] font-medium transition-colors",
                  active
                    ? "text-foreground"
                    : "text-muted-foreground hover:bg-white/[0.04] hover:text-foreground",
                )}
              >
                {active && (
                  <motion.span
                    layoutId="sidebar-active"
                    className="absolute inset-0 rounded-lg bg-white/[0.07]"
                    transition={ACTIVE_SPRING}
                  />
                )}
                <Icon className="relative z-10 size-4 shrink-0" />
                <span className="relative z-10 truncate">{item.label}</span>
              </button>
            </li>
          );
        })}
      </ul>

      <div className="mx-2 my-2 h-px shrink-0 bg-border/60" />

      <div className="flex items-center justify-between pb-1.5 pl-3 pr-1">
        <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/60">
          Content
        </span>
        <div className="flex items-center gap-0.5">
          <Button
            variant="ghost"
            size="icon"
            onClick={onSearch}
            aria-label="Tìm nội dung"
            aria-haspopup="dialog"
            title="Tìm nội dung (Ctrl/⌘+K)"
            className="size-7 text-muted-foreground/70 hover:text-foreground"
          >
            <Search className="size-3.5" />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            onClick={() => void importFile()}
            aria-label="Nhập tệp"
            title="Nhập tệp .txt hoặc .md"
            className="size-7 text-muted-foreground/70 hover:text-foreground"
          >
            <Upload className="size-3.5" />
          </Button>
        </div>
      </div>

      <ul className="mn-scroll flex min-h-0 flex-1 flex-col gap-0.5 overflow-y-auto">
        {library.scripts.map((item) => {
          const selected = screen === "content" && item.id === library.current;
          return (
            <li key={item.id}>
              <button
                type="button"
                onClick={() => open(item.id)}
                aria-current={selected ? "true" : undefined}
                title={item.title}
                className={cn(
                  "flex h-9 w-full items-center gap-2.5 rounded-lg px-3 text-left text-[13px] transition-colors",
                  selected
                    ? "bg-white/[0.07] text-foreground"
                    : "text-muted-foreground hover:bg-white/[0.04] hover:text-foreground",
                )}
              >
                <FileText
                  className={cn(
                    "size-4 shrink-0",
                    selected ? "text-primary" : "text-muted-foreground/70",
                  )}
                />
                <span className="min-w-0 truncate">{item.title}</span>
              </button>
            </li>
          );
        })}
        {library.ready && library.scripts.length === 0 && (
          <li className="px-3 py-2 text-[12.5px] text-muted-foreground/70">Chưa có nội dung</li>
        )}
      </ul>

      {library.error && (
        <p className="mt-1 px-2 text-xs leading-relaxed text-destructive" role="alert">
          {library.error}
        </p>
      )}

      <div className="mt-1 shrink-0 border-t border-border/60 py-2">
        <button
          type="button"
          onClick={() => onNavigate("settings")}
          aria-current={screen === "settings" ? "page" : undefined}
          title="Settings"
          className={cn(
            "flex h-9 w-full items-center gap-2.5 rounded-lg px-3 text-[13px] font-medium transition-colors",
            screen === "settings"
              ? "bg-white/[0.07] text-foreground"
              : "text-muted-foreground hover:bg-white/[0.04] hover:text-foreground",
          )}
        >
          <Settings2 className="size-4 shrink-0" />
          <span>Settings</span>
        </button>
      </div>
    </nav>
  );
}
