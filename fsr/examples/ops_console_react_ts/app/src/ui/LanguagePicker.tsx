import { Link, useLocale } from "@snapfire/fsr-client/react";

/** Two document loads: the default locale prefixed, so the choice is remembered, and French under its prefix. */
export function LanguagePicker() {
  const locale = useLocale();
  return (
    <div className="segmented" role="radiogroup" aria-label="Language">
      <Link href="/en_US/settings" full className={locale === "en_US" ? "seg seg-on" : "seg"}>
        English
      </Link>
      <Link href="/fr_FR/settings" full className={locale === "fr_FR" ? "seg seg-on" : "seg"}>
        Français
      </Link>
    </div>
  );
}
