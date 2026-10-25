import { lazy } from "react";
import { useStore } from "@snapfire/fsr/client/react";

import { probeCount } from "@src/store";

const LazyPart = lazy(() => import("@src/ui/LazyPart"));

export default function LazyProbe({ label }: { label: string }) {
  const [count] = useStore(probeCount, 0);
  return (
    <section className="lazy-probe" data-owner="react">
      <h3 className="label">{label}</h3>
      <p className="held">{count}</p>
      <LazyPart />
    </section>
  );
}
