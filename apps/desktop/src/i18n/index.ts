import { useSyncExternalStore } from "react";
import { en } from "./en";
import { vi } from "./vi";

/** The languages the app's own text is written in. Vietnamese first, as the project is. */
export type Lang = "vi" | "en";

/** Every piece of text. Vietnamese is the source: a key missing from `en` fails the type check. */
export type Key = keyof typeof vi;

const DICTIONARIES: Record<Lang, Record<Key, string>> = { vi, en };

let current: Lang = "vi";
const listeners = new Set<() => void>();

export function getLanguage(): Lang {
  return current;
}

/** Changes the language of every `t` call and re-renders the components that use `useT`. */
export function setLanguage(lang: Lang): void {
  if (lang === current) return;
  current = lang;
  document.documentElement.lang = lang;
  listeners.forEach((listener) => listener());
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => void listeners.delete(listener);
}

export type Params = Record<string, string | number>;

/**
 * The text for `key` in the current language. `{name}` in the text is replaced by `params.name`.
 * Components call [`useT`] so they draw again when the language changes; code outside a
 * component (an error message built in a hook) may call this directly.
 */
export function t(key: Key, params?: Params): string {
  const text = DICTIONARIES[current][key] ?? vi[key];
  if (!params) return text;
  return text.replace(/\{(\w+)\}/g, (whole, name: string) => (name in params ? String(params[name]) : whole));
}

/** `t`, and the component draws again when the language changes. */
export function useT(): typeof t {
  useSyncExternalStore(subscribe, getLanguage);
  return t;
}

export function useLanguage(): Lang {
  return useSyncExternalStore(subscribe, getLanguage);
}

/** The locale for dates and numbers in the current language. */
export function locale(): string {
  return current === "vi" ? "vi-VN" : "en-US";
}
