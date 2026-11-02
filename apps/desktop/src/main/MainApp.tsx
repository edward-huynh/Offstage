import { useState } from "react";
import { useLibrary } from "../useLibrary";
import { useOnboarding } from "../useOnboarding";
import { useSidebarCollapsed } from "../useSidebarCollapsed";
import { useWorkspaces } from "../useWorkspaces";
import { cn } from "@/lib/utils";
import { Content } from "./Content";
import { Home } from "./Home";
import { Knowledge } from "./Knowledge";
import { Onboarding } from "./Onboarding";
import { SearchDialog } from "./SearchDialog";
import { Settings } from "./Settings";
import { Sidebar } from "./Sidebar";
import { Workspaces } from "./Workspaces";

export type Screen = "home" | "content" | "knowledge" | "workspaces" | "settings";

const TITLES: Record<Screen, string> = {
  home: "Home",
  content: "Content",
  knowledge: "Knowledge",
  workspaces: "Workspace",
  settings: "Settings",
};

/** The main window: a sidebar, a title bar and one screen at a time. */
export function MainApp() {
  const [screen, setScreen] = useState<Screen>("home");
  const [collapsed, toggleSidebar] = useSidebarCollapsed();
  const library = useLibrary();
  const workspaces = useWorkspaces();
  const onboarding = useOnboarding();
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
        workspaces={workspaces}
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
          {screen === "knowledge" && <Knowledge workspaceName={workspaces.active?.name} />}
          {screen === "workspaces" && <Workspaces state={workspaces} />}
          {screen === "settings" && <Settings />}
        </main>
      </div>
      {onboarding.pending && (
        <Onboarding onDone={() => void onboarding.finish()} onOpenSettings={() => setScreen("settings")} />
      )}
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
