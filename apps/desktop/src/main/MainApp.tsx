import { useEffect, useState } from "react";
import { useLibrary } from "../useLibrary";
import { useSidebarCollapsed } from "../useSidebarCollapsed";
import { Content } from "./Content";
import { Home } from "./Home";
import { SearchDialog } from "./SearchDialog";
import { Sidebar } from "./Sidebar";

export type Screen = "home" | "content" | "knowledge" | "settings";

const TITLES: Record<Screen, string> = {
  home: "Home",
  content: "Content",
  knowledge: "Knowledge",
  settings: "Settings",
};

/** Stands in for a screen that has not been built yet. */
function ComingSoon() {
  return (
    <section className="mn-screen">
      <p className="mn-lead">Màn hình này đang được làm.</p>
    </section>
  );
}

/** The main window: a sidebar, a title bar and one screen at a time. */
export function MainApp() {
  const [screen, setScreen] = useState<Screen>("home");
  const [collapsed, toggleSidebar] = useSidebarCollapsed();
  const library = useLibrary();
  const [searchOpen, setSearchOpen] = useState(false);

  // Ctrl/Cmd+K opens the search from anywhere in the window.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setSearchOpen((open) => !open);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const openTitle = screen === "content" ? library.scripts.find((s) => s.id === library.current)?.title : null;

  return (
    <div className={collapsed ? "mn-app is-collapsed" : "mn-app"}>
      <Sidebar
        screen={screen}
        onNavigate={setScreen}
        library={library}
        collapsed={collapsed}
        onToggle={toggleSidebar}
        onSearch={() => setSearchOpen(true)}
      />
      <div className="mn-pane">
        <header className="mn-header" data-tauri-drag-region>
          <h1 className="mn-header-title" data-tauri-drag-region>
            {TITLES[screen]}
            {openTitle && <span className="mn-header-sub"> · {openTitle}</span>}
          </h1>
        </header>
        <main className="mn-main">
          {screen === "home" && <Home onNavigate={setScreen} />}
          {screen === "content" && <Content library={library} />}
          {screen === "knowledge" && <ComingSoon />}
          {screen === "settings" && <ComingSoon />}
        </main>
      </div>
      <SearchDialog
        open={searchOpen}
        onClose={() => setSearchOpen(false)}
        onPick={(id) => {
          setScreen("content");
          void library.select(id);
        }}
      />
    </div>
  );
}
