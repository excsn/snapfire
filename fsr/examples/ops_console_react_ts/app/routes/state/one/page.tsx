import { t } from "@snapfire/fsr/client";

export default function One() {
  return (
    <div className="card state-card">
      <h2>{t("state.one_title")}</h2>
      <p>{t("state.one_body")}</p>
    </div>
  );
}
