import { useContext } from "react";

import { DensityContext } from "@src/planner/density";

export default function DensityLabel({ place }: { place: string }) {
  const density = useContext(DensityContext);
  return <span className={`density-${place}`}>{density}</span>;
}
