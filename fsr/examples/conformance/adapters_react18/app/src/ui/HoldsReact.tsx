import { useState } from "react";
import { Place } from "@snapfire/fsr/components";

export default function HoldsReact() {
  const [n, setN] = useState(0);
  return (
    <section className="holds" data-holder="react">
      <button className="again" onClick={() => setN(n + 1)}>
        {n}
      </button>
      <Place name="react-place" />
    </section>
  );
}
