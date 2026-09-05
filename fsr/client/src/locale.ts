/** The document's locale, as the application spells it: read from `<html data-sf-locale>` at boot, replaced by the `L` row of every payload a navigation applies. */

type LocaleListener = (tag: string) => void;

let current = "";
const listeners = new Set<LocaleListener>();

/** The locale the document is in, or an empty string before any document says. */
export function currentLocale(): string {
  return current;
}

/** Calls `listener` whenever the document's locale changes; the returned function stops it. */
export function subscribeLocale(listener: LocaleListener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** Makes `tag` the document's locale: `<html lang>` in its BCP 47 spelling, `data-sf-locale` in the application's, and every listener told. Same tag, nothing happens. */
export function setLocale(tag: string): void {
  if (tag === current) return;
  current = tag;
  if (typeof document !== "undefined") {
    document.documentElement.setAttribute("lang", tag.replace(/_/g, "-"));
    document.documentElement.setAttribute("data-sf-locale", tag);
  }
  for (const listener of Array.from(listeners)) listener(tag);
}

/** Reads the locale the server wrote on the document. Nothing written leaves the current one. */
export function adoptLocale(): void {
  if (typeof document === "undefined") return;
  const tag = document.documentElement.getAttribute("data-sf-locale");
  if (tag) setLocale(tag);
}
