import { t } from "@snapfire/fsr-client/std";

export function TipList() {
  return (
    <details className="tips">
      <summary>{t("tips.title")}</summary>
      <ul>
        <li>{t("tips.agent")}</li>
        <li>{t("tips.peek")}</li>
        <li>{t("tips.drawer")}</li>
        <li>{t("tips.alert")}</li>
        <li>{t("tips.ack")}</li>
      </ul>
    </details>
  );
}
