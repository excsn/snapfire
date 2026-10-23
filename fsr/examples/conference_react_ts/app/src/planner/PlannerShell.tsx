import { Island } from "@snapfire/fsr-client/react";
import { useState, type ReactNode } from "react";

import DensityLabel from "@src/planner/DensityLabel";
import { DensityContext, type Density } from "@src/planner/density";

export default function PlannerShell({ children }: { children?: ReactNode }) {
  const [density, setDensity] = useState<Density>("comfortable");
  return (
    <DensityContext.Provider value={density}>
      <section className={`planner planner-${density}`}>
        <header className="planner-head">
          <Island>
            <DensityLabel place="head" />
          </Island>
          <button className="planner-compact" onClick={() => setDensity("compact")}>Compact</button>
        </header>
        {children}
      </section>
    </DensityContext.Provider>
  );
}
