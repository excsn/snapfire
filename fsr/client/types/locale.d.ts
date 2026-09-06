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
/** A locale's message table: dotted keys to strings, already merged over the default locale's on the server. */
export type Catalog = {
	readonly [key: string]: string;
};
/** The message table held for `tag`, or null when the server has sent none. */
export declare function catalog(tag: string): Catalog | null;
/** Holds `table` as the messages of `tag`, replacing what was held. */
export declare function setCatalog(tag: string, table: Catalog): void;
/** Reads the catalog the server embedded in the document, `<script type="application/json" data-sf-i18n="fr_FR">`. Nothing embedded leaves what is held. */
export declare function adoptCatalog(): void;
export {};
