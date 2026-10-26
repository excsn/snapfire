import { useState } from "react";
import { t } from "@snapfire/fsr/client";

/** A note beside the state pages that the reader can put away. */
export function Reminder() {
  const [open, setOpen] = useState(true);
  if (!open) return <p className="hint">{t("state.reminder_gone")}</p>;
  return (
    <p className="hint state-reminder">
      {t("state.reminder")}{" "}
      <button className="btn-small" onClick={() => setOpen(false)}>
        {t("state.dismiss")}
      </button>
    </p>
  );
}
