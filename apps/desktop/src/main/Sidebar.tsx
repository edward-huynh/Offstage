import type { Library } from "../useLibrary";
import { BookIcon, DocumentIcon, HomeIcon, ImportIcon, PlusCircleIcon, SearchIcon, SidebarIcon, SlidersIcon } from "./icons";
import type { Screen } from "./MainApp";

type Props = {
  screen: Screen;
  onNavigate: (screen: Screen) => void;
  library: Library;
  collapsed: boolean;
  onToggle: () => void;
  onSearch: () => void;
};

const NAV: Array<{ screen: Screen; label: string; icon: JSX.Element }> = [
  { screen: "home", label: "Home", icon: <HomeIcon /> },
  { screen: "content", label: "Content", icon: <DocumentIcon /> },
  { screen: "knowledge", label: "Knowledge", icon: <BookIcon /> },
];

/** The left sidebar: quick create, the screens, the list of contents, and Settings at the bottom. */
export function Sidebar({ screen, onNavigate, library, collapsed, onToggle, onSearch }: Props) {
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
    <nav className="mn-side" aria-label="Điều hướng">
      <div className="mn-side-top" data-tauri-drag-region>
        <button
          type="button"
          className="mn-icon-btn"
          onClick={onToggle}
          aria-label={collapsed ? "Mở rộng thanh bên" : "Thu gọn thanh bên"}
          aria-expanded={!collapsed}
          title={collapsed ? "Mở rộng thanh bên" : "Thu gọn thanh bên"}
        >
          <SidebarIcon />
        </button>
      </div>

      <div className="mn-brand">
        <span className="mn-brand-dot" aria-hidden />
        <span className="mn-label">Offstage</span>
      </div>

      <button type="button" className="mn-new" onClick={() => void create()} title="Content mới">
        <PlusCircleIcon />
        <span className="mn-label">Content mới</span>
      </button>

      <ul className="mn-nav-list">
        {NAV.map((item) => (
          <li key={item.screen}>
            <button
              type="button"
              className={item.screen === screen ? "mn-nav is-active" : "mn-nav"}
              aria-current={item.screen === screen ? "page" : undefined}
              onClick={() => onNavigate(item.screen)}
              title={item.label}
            >
              {item.icon}
              <span className="mn-label">{item.label}</span>
            </button>
          </li>
        ))}
      </ul>

      <div className="mn-section mn-label-block">
        <div className="mn-section-head">
          <span>Content</span>
          <span className="mn-section-actions">
            <button
              type="button"
              className="mn-icon-btn small"
              aria-label="Tìm nội dung"
              aria-haspopup="dialog"
              title="Tìm nội dung (Ctrl/⌘+K)"
              onClick={onSearch}
            >
              <SearchIcon size={16} />
            </button>
            <button type="button" className="mn-icon-btn small" aria-label="Nhập tệp" title="Nhập tệp .txt hoặc .md" onClick={() => void importFile()}>
              <ImportIcon size={16} />
            </button>
          </span>
        </div>

        <ul className="mn-recent" aria-label="Danh sách nội dung">
          {library.scripts.map((item) => {
            const isOpen = screen === "content" && item.id === library.current;
            return (
              <li key={item.id}>
                <button
                  type="button"
                  className={isOpen ? "mn-recent-item is-selected" : "mn-recent-item"}
                  aria-current={isOpen ? "true" : undefined}
                  onClick={() => open(item.id)}
                  title={item.title}
                >
                  {item.title}
                </button>
              </li>
            );
          })}
          {library.ready && library.scripts.length === 0 && <li className="mn-recent-empty">Chưa có nội dung</li>}
        </ul>
        {library.error && (
          <p className="mn-side-error" role="alert">
            {library.error}
          </p>
        )}
      </div>

      <button
        type="button"
        className={screen === "settings" ? "mn-profile is-active" : "mn-profile"}
        aria-current={screen === "settings" ? "page" : undefined}
        onClick={() => onNavigate("settings")}
        title="Settings"
      >
        <span className="mn-avatar" aria-hidden>
          <SlidersIcon size={16} />
        </span>
        <span className="mn-label">Settings</span>
      </button>
    </nav>
  );
}
