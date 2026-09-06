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

/** A locale's message table: dotted keys to strings, already merged over the default locale's on the server. */
export type Catalog = { readonly [key: string]: string };

const catalogs = new Map<string, Catalog>();

/** The message table held for `tag`, or null when the server has sent none. */
export function catalog(tag: string): Catalog | null {
  return catalogs.get(tag) ?? null;
}

/** Holds `table` as the messages of `tag`, replacing what was held. */
export function setCatalog(tag: string, table: Catalog): void {
  catalogs.set(tag, table);
}

/** Reads the catalog the server embedded in the document, `<script type="application/json" data-sf-i18n="fr_FR">`. Nothing embedded leaves what is held. */
export function adoptCatalog(): void {
  if (typeof document === "undefined") return;
  const script = document.querySelector("script[data-sf-i18n]");
  const tag = script?.getAttribute("data-sf-i18n");
  if (!script || !tag) return;
  try {
    setCatalog(tag, JSON.parse(script.textContent ?? "{}") as Catalog);
  } catch {
    // A catalog that does not parse is left out; `t` answers the key itself.
  }
}
