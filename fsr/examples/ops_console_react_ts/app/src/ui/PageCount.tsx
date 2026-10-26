import { useState } from "react";
import { t } from "@snapfire/fsr/client";

/** A count of this page's own, which starts over each time the page is rendered while the layout's count carries on. */
export function PageCount() {
  const [count, setCount] = useState(0);
  return (
    <button className="counter" onClick={() => setCount(count + 1)} aria-label="page count">
      {t("state.page_clicked", { count })}
    </button>
  );
}
