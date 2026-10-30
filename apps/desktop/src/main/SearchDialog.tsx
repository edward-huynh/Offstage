import { useEffect, useId, useRef, useState } from "react";
import { errorMessage, searchScripts, type ScriptHit } from "../tauri";
import { CloseIcon, SearchIcon } from "./icons";

const DEBOUNCE_MS = 150;

type Props = {
  open: boolean;
  onClose: () => void;
  /** Called with the id of the content the user picked. The dialog closes itself afterwards. */
  onPick: (id: string) => void;
};

/**
 * A search box centred over the window, with the app blurred behind it. It searches the titles
 * and the text of the contents, and picking a result opens that content.
 */
export function SearchDialog({ open, onClose, onPick }: Props) {
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<ScriptHit[]>([]);
  const [active, setActive] = useState(0);
  const [error, setError] = useState<string | null>(null);
  /** True once `hits` answers the current query, so "no results" is not shown while typing. */
  const [answered, setAnswered] = useState(false);
  const listId = useId();
  const returnFocusTo = useRef<Element | null>(null);
  const input = useRef<HTMLInputElement>(null);
  const box = useRef<HTMLDivElement>(null);

  // Focus the box when it opens, and give focus back to whatever opened it when it closes.
  // The input is focused here rather than with `autoFocus`, which would run first and hide the opener.
  useEffect(() => {
    if (!open) return;
    returnFocusTo.current = document.activeElement;
    input.current?.focus();
    return () => (returnFocusTo.current as HTMLElement | null)?.focus?.();
  }, [open]);

  useEffect(() => {
    if (open) return;
    setQuery("");
    setHits([]);
    setActive(0);
    setError(null);
    setAnswered(false);
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
    const timer = window.setTimeout(() => {
      searchScripts(text)
        .then((found) => {
          if (cancelled) return;
          setHits(found);
          setActive(0);
          setError(null);
          setAnswered(true);
        })
        .catch((e) => {
          if (cancelled) return;
          setError(errorMessage(e));
          setAnswered(false);
        });
    }, DEBOUNCE_MS);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [query, open]);

  if (!open) return null;

  const pick = (hit: ScriptHit) => {
    onPick(hit.id);
    onClose();
  };

  const onInputKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive((i) => Math.min(i + 1, hits.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter") {
      const hit = hits[active];
      if (hit) {
        e.preventDefault();
        pick(hit);
      }
    }
  };

  const text = query.trim();

  return (
    <div
      className="mn-search-backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        ref={box}
        className="mn-search-box"
        role="dialog"
        aria-modal="true"
        aria-label="Tìm nội dung"
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.preventDefault();
            onClose();
            return;
          }
          // Keep Tab inside the modal: the window behind it is still marked aria-modal.
          if (e.key !== "Tab" || !box.current) return;
          const focusable = box.current.querySelectorAll<HTMLElement>(
            'button:not([disabled]):not([tabindex="-1"]), input, [href], select, textarea',
          );
          if (focusable.length === 0) return;
          const first = focusable[0];
          const last = focusable[focusable.length - 1];
          if (e.shiftKey && document.activeElement === first) {
            e.preventDefault();
            last.focus();
          } else if (!e.shiftKey && document.activeElement === last) {
            e.preventDefault();
            first.focus();
          }
        }}
      >
        <div className="mn-search-row">
          <SearchIcon size={20} />
          <input
            className="mn-search-input"
            ref={input}
            value={query}
            placeholder="Tìm trong nội dung…"
            aria-label="Tìm trong nội dung"
            role="combobox"
            aria-autocomplete="list"
            aria-expanded={hits.length > 0}
            aria-controls={listId}
            aria-activedescendant={hits[active] ? `${listId}-${active}` : undefined}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={onInputKeyDown}
          />
          <span className="mn-search-sep" aria-hidden />
          <button type="button" className="mn-icon-btn" aria-label="Đóng" title="Đóng (Esc)" onClick={onClose}>
            <CloseIcon />
          </button>
        </div>

        {hits.length > 0 && (
          <ul id={listId} role="listbox" className="mn-search-results" aria-label="Kết quả">
            {hits.map((hit, index) => (
              <li key={hit.id} id={`${listId}-${index}`} role="option" aria-selected={index === active}>
                <button
                  type="button"
                  tabIndex={-1}
                  className={index === active ? "mn-search-hit is-active" : "mn-search-hit"}
                  onMouseEnter={() => setActive(index)}
                  onClick={() => pick(hit)}
                >
                  <span className="mn-search-hit-title">{hit.title}</span>
                  {hit.snippet && <span className="mn-search-hit-snippet">{hit.snippet}</span>}
                </button>
              </li>
            ))}
          </ul>
        )}
        {answered && text && hits.length === 0 && !error && (
          <p className="mn-search-note">Không thấy kết quả cho “{text}”.</p>
        )}
        {error && (
          <p className="mn-search-note is-error" role="alert">
            {error}
          </p>
        )}
      </div>
    </div>
  );
}
