import { useEffect, useState } from "react";
import {
  errorMessage,
  getPlaySession,
  listScripts,
  onPlayStarted,
  onPlayStopped,
  readScript,
  type PlaySession,
} from "./tauri";

/**
 * The session the overlay is playing, and the text of the content when it is a content session.
 * Rust holds the session. This asks for it when the overlay loads and follows the events after
 * that, so a session started from the main window shows up here.
 */
export function usePlay() {
  const [session, setSession] = useState<PlaySession | null>(null);
  const [body, setBody] = useState("");
  const [title, setTitle] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    const unlisteners: Array<() => void> = [];
    const keep = (subscription: Promise<() => void>) =>
      subscription.then((off) => {
        if (cancelled) off();
        else unlisteners.push(off);
      });

    void getPlaySession().then((current) => {
      if (!cancelled) setSession(current);
    });
    keep(onPlayStarted(setSession));
    keep(onPlayStopped(() => setSession(null)));

    return () => {
      cancelled = true;
      unlisteners.forEach((off) => off());
    };
  }, []);

  const contentId = session?.mode === "content" ? session.content_id : null;
  useEffect(() => {
    setError(null);
    if (!contentId) {
      setBody("");
      setTitle(null);
      return;
    }
    let cancelled = false;
    Promise.all([readScript(contentId), listScripts()])
      .then(([text, list]) => {
        if (cancelled) return;
        setBody(text);
        setTitle(list.find((s) => s.id === contentId)?.title ?? null);
      })
      .catch((e) => {
        if (!cancelled) setError(errorMessage(e));
      });
    return () => {
      cancelled = true;
    };
  }, [contentId]);

  return { session, body, title, error };
}
