import { get, optimistic } from "@snapfire/fsr-client";
import { useStore } from "@snapfire/fsr-client/react";
import { t } from "@snapfire/fsr-client/std";

import { actions, type Agent } from "@generated/client";
import { density, watching } from "@src/store";
import { LanguagePicker } from "@src/ui/LanguagePicker";

export function SettingsPanel({ watched, locales }: { watched: Agent[]; locales: string[] }) {
  const [rows] = useStore(density, "comfortable");

  async function unwatch(id: bigint | number): Promise<void> {
    await optimistic(watching, (get(watching) ?? 1) - 1, () => actions.settings.unwatchAgent({ agent_id: id }));
  }

  async function choose(next: string): Promise<void> {
    await optimistic(density, next, () => actions.settings.setDensity({ density: next }));
  }

  return (
    <div className="settings">
      <section>
        <h2>{t("settings.language")}</h2>
        <LanguagePicker locales={locales} />
      </section>
      <section>
        <h2>{t("settings.rows")}</h2>
        <div className="segmented" role="radiogroup" aria-label={t("settings.rows_label")}>
          <button className={rows === "comfortable" ? "seg seg-on" : "seg"} onClick={() => void choose("comfortable")}>
            {t("settings.comfortable")}
          </button>
          <button className={rows === "compact" ? "seg seg-on" : "seg"} onClick={() => void choose("compact")}>
            {t("settings.compact")}
          </button>
        </div>
      </section>
      <section>
        <h2>{t("settings.watching")}</h2>
        {watched.length === 0 ? <p className="quiet">{t("settings.none")}</p> : null}
        <ul className="watch-list">
          {watched.map((a) => (
            <li key={String(a.id)}>
              <span className="agent-name">{a.name}</span>
              <span className="agent-region">{a.region}</span>
              <button className="btn btn-small" onClick={() => void unwatch(a.id)}>
                {t("agents.unwatch")}
              </button>
            </li>
          ))}
        </ul>
      </section>
    </div>
  );
}
