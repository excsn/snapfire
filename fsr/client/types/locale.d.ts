/** The document's locale, as the application spells it: read from `<html data-sf-locale>` at boot, replaced by the `L` row of every payload a navigation applies. */
type LocaleListener = (tag: string) => void;
/** The locale the document is in, or an empty string before any document says. */
export declare function currentLocale(): string;
/** Calls `listener` whenever the document's locale changes; the returned function stops it. */
export declare function subscribeLocale(listener: LocaleListener): () => void;
/** Makes `tag` the document's locale: `<html lang>` in its BCP 47 spelling, `data-sf-locale` in the application's, and every listener told. Same tag, nothing happens. */
export declare function setLocale(tag: string): void;
/** Reads the locale the server wrote on the document. Nothing written leaves the current one. */
export declare function adoptLocale(): void;
export {};
