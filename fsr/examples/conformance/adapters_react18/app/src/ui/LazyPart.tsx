import { useEffect } from "react";
import { useStore } from "@snapfire/fsr-client/react";

import { rendered } from "@src/probes";
import { probeCount } from "@src/store";

export default function LazyPart() {
  const [count] = useStore(probeCount, 0);
  useEffect(() => {
    rendered("react-lazy");
  });
  return <p className="lazy-part">{count}</p>;
}
