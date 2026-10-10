import { t } from "@snapfire/fsr-client/std";

export default function AlertsLoading() {
  return (
    <div className="alerts">
      <h2>{t("alerts.title")}</h2>
      <p className="skeleton skeleton-line" />
      <p className="skeleton skeleton-line" />
    </div>
  );
}
