import { useEffect, useState } from "react";
import { BookOpen, FileText, Home, Settings2, type LucideIcon } from "lucide-react";
import {
  CommandPalette,
  type CommandItem,
} from "@/components/motion/command-palette";
import { errorMessage, searchScripts, type ScriptHit, type ScriptMeta } from "../tauri";
import type { Screen } from "./MainApp";

const DEBOUNCE_MS = 150;

const NAV_ITEMS: Array<{ id: Screen; label: string; icon: LucideIcon }> = [
  { id: "home", label: "Home", icon: Home },
  { id: "content", label: "Content", icon: FileText },
  { id: "knowledge", label: "Knowledge", icon: BookOpen },
  { id: "settings", label: "Settings", icon: Settings2 },
];

type Props = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Recent contents, shown when the query is empty. */
  scripts: ScriptMeta[];
  onNavigate: (screen: Screen) => void;
  /** Called with the id of the content the user picked. The palette closes itself afterwards. */
  onPick: (id: string) => void;
};

/**
 * The ⌘K palette. With no query it lists the screens and recent contents; typing
 * searches the titles and text of the contents through Rust.
 */
export function SearchDialog({ open, onOpenChange, scripts, onNavigate, onPick }: Props) {
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<ScriptHit[]>([]);
  const [answered, setAnswered] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // The palette clears its own field when it opens; keep the results in step.
  useEffect(() => {
    if (!open) return;
    setQuery("");
    setHits([]);
    setAnswered(false);
    setError(null);
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const text = query.trim();
    if (!text) {
      setHits([]);
      setAnswered(false);
      setError(null);
      return;
    }
    let cancelled = false;
    setAnswered(false);
    const timer = window.setTimeout(() => {
      searchScripts(text)
        .then((found) => {
          if (cancelled) return;
          setHits(found);
          setAnswered(true);
          setError(null);
        })
        .catch((e) => {
          if (cancelled) return;
          setHits([]);
          setAnswered(true);
          setError(errorMessage(e));
        });
    }, DEBOUNCE_MS);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [query, open]);

  const text = query.trim();

  const items: CommandItem[] = text
    ? hits.map((hit) => ({
        id: `hit-${hit.id}`,
        label: hit.title,
        group: "Nội dung",
        icon: FileText,
        badge: hit.snippet ? (
          <span className="hidden max-w-[220px] truncate text-xs text-muted-foreground sm:inline">
            {hit.snippet}
          </span>
        ) : undefined,
        onSelect: () => onPick(hit.id),
      }))
    : [
        ...NAV_ITEMS.map((item) => ({
          id: `nav-${item.id}`,
          label: item.label,
          group: "Điều hướng",
          icon: item.icon,
          onSelect: () => onNavigate(item.id),
        })),
        ...scripts.slice(0, 8).map((script) => ({
          id: `script-${script.id}`,
          label: script.title,
          group: "Content gần đây",
          icon: FileText,
          onSelect: () => onPick(script.id),
        })),
      ];

  const emptyMessage = error
    ? error
    : text && !answered
      ? "Đang tìm…"
      : text
        ? `Không thấy kết quả cho “${text}”.`
        : "Chưa có nội dung nào.";

  return (
    <CommandPalette
      open={open}
      onOpenChange={onOpenChange}
      items={items}
      onQueryChange={setQuery}
      placeholder="Tìm trong nội dung…"
      emptyMessage={emptyMessage}
    />
  );
}
