import { useCallback, useEffect, useRef, useState } from "react";
import {
  assistantStatus,
  clearAssistantKey,
  errorMessage,
  getAssistantConfig,
  listAssistantModels,
  onAnswerDone,
  onAnswerToken,
  onListeningError,
  onQuestionDetected,
  setAssistantConfig,
  setAssistantKey,
  testAssistantConnection,
  type AssistantConfig,
  type AssistantStatus,
} from "./tauri";

export type AnswerPhase = "idle" | "streaming" | "done" | "failed";
export type Answer = { question: string; text: string; phase: AnswerPhase };

const NO_ANSWER: Answer = { question: "", text: "", phase: "idle" };

/**
 * The status of the assistant, read again whenever `refreshKey` changes. Settings are edited in
 * another window, so a screen that stays open passes something that changes when it starts again.
 */
export function useAssistantStatus(refreshKey?: unknown) {
  const [status, setStatus] = useState<AssistantStatus | null>(null);
  useEffect(() => {
    let cancelled = false;
    void assistantStatus()
      .then((s) => {
        if (!cancelled) setStatus(s);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [refreshKey]);
  return status;
}

/**
 * The answer being written for the latest question. Rust does the work; this only collects
 * the events. Tokens arrive many times a second, so they are gathered and shown once per frame.
 */
export function useAnswer(refreshKey?: unknown) {
  const [answer, setAnswer] = useState<Answer>(NO_ANSWER);
  const status = useAssistantStatus(refreshKey);

  useEffect(() => {
    let cancelled = false;
    const unlisteners: Array<() => void> = [];
    const keep = (subscription: Promise<() => void>) =>
      subscription.then((off) => {
        if (cancelled) off();
        else unlisteners.push(off);
      });

    let pending = "";
    let frame: number | undefined;
    const flush = () => {
      frame = undefined;
      const text = pending;
      pending = "";
      if (text) setAnswer((a) => (a.phase === "streaming" ? { ...a, text: a.text + text } : a));
    };

    keep(
      onQuestionDetected((question) => {
        pending = "";
        setAnswer({ question, text: "", phase: "streaming" });
      }),
    );
    keep(
      onAnswerToken((token) => {
        pending += token;
        frame ??= requestAnimationFrame(flush);
      }),
    );
    keep(
      onAnswerDone(() => {
        if (frame !== undefined) cancelAnimationFrame(frame);
        const rest = pending;
        pending = "";
        setAnswer((a) => ({ ...a, text: a.text + rest, phase: "done" }));
      }),
    );
    // Any pipeline error while an answer is being written ends it, so the panel never waits forever.
    keep(onListeningError(() => setAnswer((a) => (a.phase === "streaming" ? { ...a, phase: "failed" } : a))));

    return () => {
      cancelled = true;
      if (frame !== undefined) cancelAnimationFrame(frame);
      unlisteners.forEach((off) => off());
    };
  }, []);

  return { answer, status };
}

/** The assistant section of Settings: the saved choice, the key, the model list and a connection test. */
export function useAssistantSettings() {
  const [config, setConfig] = useState<AssistantConfig | null>(null);
  const [status, setStatus] = useState<AssistantStatus | null>(null);
  const [models, setModels] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<"key" | "models" | "test" | null>(null);
  const [connection, setConnection] = useState<"ok" | null>(null);
  const latest = useRef<AssistantConfig | null>(null);

  const refresh = useCallback(async () => {
    setStatus(await assistantStatus());
  }, []);

  useEffect(() => {
    let cancelled = false;
    Promise.all([getAssistantConfig(), assistantStatus()])
      .then(([c, s]) => {
        if (cancelled) return;
        latest.current = c;
        setConfig(c);
        setStatus(s);
      })
      .catch((e) => {
        if (!cancelled) setError(errorMessage(e));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  /** Saves a change at once. A refusal (turning it on with an unusable provider) puts the old choice back. */
  const change = useCallback(
    async (patch: Partial<AssistantConfig>) => {
      const before = latest.current;
      if (!before) return;
      const next = { ...before, ...patch };
      setError(null);
      setConnection(null);
      latest.current = next;
      setConfig(next);
      try {
        const stored = await setAssistantConfig(next);
        latest.current = stored;
        setConfig(stored);
        await refresh();
      } catch (e) {
        latest.current = before;
        setConfig(before);
        setError(errorMessage(e));
      }
    },
    [refresh],
  );

  /** Another provider has another key and another list of models. */
  const changeProvider = useCallback(
    async (provider_id: string) => {
      setModels([]);
      await change({ provider_id, model: null });
    },
    [change],
  );

  const saveKey = useCallback(
    async (key: string) => {
      const provider = latest.current?.provider_id;
      if (!provider || !key.trim()) return;
      setError(null);
      setConnection(null);
      setBusy("key");
      try {
        await setAssistantKey(provider, key);
        await refresh();
      } catch (e) {
        setError(errorMessage(e));
      } finally {
        setBusy(null);
      }
    },
    [refresh],
  );

  const removeKey = useCallback(async () => {
    const provider = latest.current?.provider_id;
    if (!provider) return;
    setError(null);
    setConnection(null);
    setBusy("key");
    try {
      await clearAssistantKey(provider);
      await refresh();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(null);
    }
  }, [refresh]);

  const loadModels = useCallback(async () => {
    setError(null);
    setBusy("models");
    try {
      setModels(await listAssistantModels());
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(null);
    }
  }, []);

  const test = useCallback(async () => {
    setError(null);
    setConnection(null);
    setBusy("test");
    try {
      await testAssistantConnection();
      setConnection("ok");
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(null);
    }
  }, []);

  return { config, status, models, error, busy, connection, change, changeProvider, saveKey, removeKey, loadModels, test };
}
