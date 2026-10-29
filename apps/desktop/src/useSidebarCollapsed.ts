import { useCallback, useState } from "react";

const KEY = "offstage.sidebar.collapsed";

function read(): boolean {
  try {
    return window.localStorage.getItem(KEY) === "1";
  } catch {
    // Storage can be blocked, and the sidebar then simply starts open.
    return false;
  }
}

/** Whether the sidebar is folded to icons. Remembered between launches when storage allows. */
export function useSidebarCollapsed() {
  const [collapsed, setCollapsed] = useState(read);

  const toggle = useCallback(() => {
    setCollapsed((current) => {
      try {
        window.localStorage.setItem(KEY, current ? "0" : "1");
      } catch {
        // Not remembering it is harmless.
      }
      return !current;
    });
  }, []);

  return [collapsed, toggle] as const;
}
