import { t } from "@snapfire/fsr-client/std";

export default function PickOne() {
  return (
    <div className="pick">
      <p>{t("agents.pick")}</p>
    </div>
  );
}
