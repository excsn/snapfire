import { t } from "@snapfire/fsr-client/std";

import type { SettingsProps } from "@generated/client";
import { SettingsPanel } from "@src/ui/SettingsPanel";

export default function SettingsPage({ watched }: SettingsProps) {
  return (
    <div className="page">
      <h1>{t("settings.title")}</h1>
      <p className="lede">{t("settings.lede")}</p>
      <SettingsPanel watched={watched} />
    </div>
  );
}
