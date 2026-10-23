import { useContext } from "react";

import { DensityContext } from "@src/planner/density";

export default function Later() {
  const density = useContext(DensityContext);
  return <p className="planner-later">{density}</p>;
}
