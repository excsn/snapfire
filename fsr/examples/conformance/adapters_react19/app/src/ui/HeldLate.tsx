import { useEffect } from "react";
import { useStore } from "@snapfire/fsr-client/react";

import { rendered } from "@src/probes";
import { probeCount } from "@src/store";

export default function HeldLate() {
  const [count] = useStore(probeCount, 0);
  useEffect(() => {
    rendered("react-late");
  });
  return <p className="late">{count}</p>;
}
