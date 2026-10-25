import { t } from "@snapfire/fsr/client";

import type { StateOneProps } from "@generated/client";

export default function One({ tally }: StateOneProps) {
  return (
    <div className="card state-card">
      <h2>{t("state.one_title")}</h2>
      <p>{t("state.one_body")}</p>
      <div className="state-tally" id="state-tally">
        <button
          className="counter"
          aria-label="tally"
          hx-get={`/state/one?__fragment&tally=${tally + 1}`}
          hx-target="#state-tally"
          hx-select="#state-tally"
          hx-swap="outerHTML"
        >
          {t("state.tally", { count: tally })}
        </button>
        <p className="hint">{t("state.tally_hint")}</p>
      </div>
    </div>
  );
}
