import { Link } from "@snapfire/fsr/components";
import { t } from "@snapfire/fsr-client/std";

import type { SettingsProps } from "@generated/client";
import { SettingsPanel } from "@src/ui/SettingsPanel";

export default function SettingsDrawer({ watched }: SettingsProps) {
  return (
    <div className="drawer">
      <h2>{t("settings.title")}</h2>
      <SettingsPanel watched={watched} />
      <p className="drawer-foot">
        <Link href="/settings" full>
          {t("settings.open_page")}
        </Link>
      </p>
    </div>
  );
}
