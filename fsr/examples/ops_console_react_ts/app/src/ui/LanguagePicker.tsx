import { localePath } from "@snapfire/fsr-client";
import { useLocale } from "@snapfire/fsr-client/react";
import { t } from "@snapfire/fsr-client/std";

const NAMES: Record<string, string> = { en_US: "English", fr_FR: "Français" };

/** A document load either way, since the whole tree renders in the new locale. The href is the literal fallback a browser with no JavaScript follows; the click sends the reader to the page they are actually on under the other prefix, which is not always the page this control lives on. */
export function LanguagePicker({ locales }: { locales: string[] }) {
  const locale = useLocale();

  function go(tag: string, event: { preventDefault: () => void }): void {
    event.preventDefault();
    window.location.assign(localePath(tag));
  }

  return (
    <div className="segmented" role="radiogroup" aria-label={t("settings.language")}>
      {locales.map((tag) => (
        <a key={tag} href={`/${tag}/settings`} data-sf-native className={locale === tag ? "seg seg-on" : "seg"} onClick={(e) => go(tag, e)}>
          {NAMES[tag] ?? tag}
        </a>
      ))}
    </div>
  );
}
