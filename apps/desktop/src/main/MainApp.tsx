import { useState } from "react";
import { useLibrary } from "../useLibrary";
import { useSidebarCollapsed } from "../useSidebarCollapsed";
import { cn } from "@/lib/utils";
import { Content } from "./Content";
import { Home } from "./Home";
import { Knowledge } from "./Knowledge";
import { SearchDialog } from "./SearchDialog";
import { Settings } from "./Settings";
import { Sidebar } from "./Sidebar";

export type Screen = "home" | "content" | "knowledge" | "settings";

const TITLES: Record<Screen, string> = {
  home: "Home",
  content: "Content",
  knowledge: "Knowledge",
  settings: "Settings",
};

/** The main window: a sidebar, a title bar and one screen at a time. */
export function MainApp() {
  const [screen, setScreen] = useState<Screen>("home");
  const [collapsed, toggleSidebar] = useSidebarCollapsed();
  const library = useLibrary();
  const [searchOpen, setSearchOpen] = useState(false);

  const openTitle =
    screen === "content" ? library.scripts.find((s) => s.id === library.current)?.title : null;

  return (
    <div
      className={cn(
        "grid h-full grid-rows-1 text-[14px] transition-[grid-template-columns] duration-200",
        collapsed ? "grid-cols-[minmax(0,1fr)]" : "grid-cols-[252px_minmax(0,1fr)]",
      )}
    >
      <Sidebar
        screen={screen}
        onNavigate={setScreen}
        library={library}
        collapsed={collapsed}
        onToggle={toggleSidebar}
        onSearch={() => setSearchOpen(true)}
      />
      <div
        className={cn(
          "flex h-full min-h-0 min-w-0 flex-col",
          collapsed && "mn-main-collapsed",
        )}
      >
        <header
          className="mn-titlebar flex shrink-0 items-center border-b border-border px-6"
          data-tauri-drag-region
        >
          <h1
            className="flex min-w-0 items-center gap-2 text-sm"
            data-tauri-drag-region
          >
            <span className="size-1.5 shrink-0 rounded-full bg-primary" aria-hidden />
            <span className="font-semibold text-foreground">Offstage</span>
            <span className="text-muted-foreground/50">/</span>
            <span className="text-muted-foreground">{TITLES[screen]}</span>
            {openTitle && (
              <span className="min-w-0 truncate text-muted-foreground/70">· {openTitle}</span>
            )}
          </h1>
        </header>
        <main className="flex min-h-0 flex-1 flex-col overflow-hidden">
          {screen === "home" && <Home onNavigate={setScreen} />}
          {screen === "content" && <Content library={library} />}
          {screen === "knowledge" && <Knowledge />}
          {screen === "settings" && <Settings />}
        </main>
      </div>
      <SearchDialog
        open={searchOpen}
        onOpenChange={setSearchOpen}
        scripts={library.scripts}
        onNavigate={setScreen}
        onPick={(id) => {
          setScreen("content");
          void library.select(id);
        }}
      />
    </div>
  );
}
