import { optimistic } from "@snapfire/fsr-client";
import { Link } from "@snapfire/fsr-client/react";
import { t } from "@snapfire/fsr-client/std";

import { actions, type LayoutAlertsProps } from "@generated/client";
import { openAlerts } from "@src/store";
import { level } from "@src/ui/level";

export default function Alerts({ alerts }: LayoutAlertsProps) {
  async function ack(id: bigint | number, open: number): Promise<void> {
    await optimistic(openAlerts, open - 1, () => actions.layout.alerts.ackAlert({ alert_id: id }));
  }

  return (
    <div className="alerts">
      <h2>{t("alerts.title")}</h2>
      {alerts.length === 0 ? <p className="quiet">{t("alerts.none")}</p> : null}
      <ul>
        {alerts.map((a) => (
          <li key={String(a.id)} className={level(a.level)}>
            <span className="alert-level">{a.level}</span>
            <span className="alert-text">{a.text}</span>
            <Link href={`/agents/${a.agent_id}`} className="alert-open" title={t("alerts.open_title")}>
              {t("alerts.open")}
            </Link>
            <button className="btn btn-small" onClick={() => void ack(a.id, alerts.length)}>
              {t("alerts.ack")}
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
