import { useState } from "react";
import { useStore } from "@snapfire/fsr-client/react";

import HeldLate from "@src/ui/HeldLate";
import { probeCount, probeOther } from "@src/store";

export default function HeldProbe({ label }: { label: string }) {
  const [count] = useStore(probeCount, 0);
  const [other] = useStore(probeOther, 0);
  const [more, setMore] = useState(false);
  return (
    <section className="held-probe" data-owner="react">
      <h3 className="label">{label}</h3>
      <p className="held">{count}</p>
      <p className="other">{other}</p>
      <button className="more" onClick={() => setMore(true)}>
        more
      </button>
      {more ? <HeldLate /> : null}
    </section>
  );
}
