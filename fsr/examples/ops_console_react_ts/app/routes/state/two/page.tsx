import { t } from "@snapfire/fsr-client/std";

export default function Two() {
  return (
    <div className="card state-card">
      <h2>{t("state.two_title")}</h2>
      <p>{t("state.two_body")}</p>
    </div>
  );
}
