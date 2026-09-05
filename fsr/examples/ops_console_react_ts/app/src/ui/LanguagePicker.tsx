import { localePath } from "@snapfire/fsr-client";
import { useLocale } from "@snapfire/fsr-client/react";

/** A document load either way, since the whole tree renders in the new locale. The href is the literal fallback a browser with no JavaScript follows; the click sends the reader to the page they are actually on under the other prefix, which is not always the page this control lives on. */
export function LanguagePicker() {
  const locale = useLocale();

  function go(tag: string, event: { preventDefault: () => void }): void {
    event.preventDefault();
    window.location.assign(localePath(tag));
  }

  return (
    <div className="segmented" role="radiogroup" aria-label="Language">
      <a href="/en_US/settings" data-sf-native className={locale === "en_US" ? "seg seg-on" : "seg"} onClick={(e) => go("en_US", e)}>
        English
      </a>
      <a href="/fr_FR/settings" data-sf-native className={locale === "fr_FR" ? "seg seg-on" : "seg"} onClick={(e) => go("fr_FR", e)}>
        Français
      </a>
    </div>
  );
}
