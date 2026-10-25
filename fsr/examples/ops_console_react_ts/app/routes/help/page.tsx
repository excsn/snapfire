import { t } from "@snapfire/fsr/client";

export default function Help() {
  return (
    <div className="page help">
      <h1>{t("help.title")}</h1>
      <p>{t("help.panels")}</p>
      <p>{t("help.streaming")}</p>
    </div>
  );
}
