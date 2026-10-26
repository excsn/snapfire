import { t } from "@snapfire/fsr/client";
import { Island } from "@snapfire/fsr/client/react";

import { PageCount } from "@src/ui/PageCount";

export default function Two() {
  return (
    <div className="card state-card">
      <h2>{t("state.two_title")}</h2>
      <p>{t("state.two_body")}</p>
      <Island when="visible">
        <PageCount />
      </Island>
    </div>
  );
}
