import { useState } from "react";

import Choice from "./Choice";

export default function Chooser() {
  const [picked, setPicked] = useState("none");
  const pear = () => setPicked("pear");
  function fig() {
    setPicked("fig");
  }
  return (
    <div className="chooser">
      <p className="picked">{picked}</p>
      <Choice label="pear" chosen={picked === "pear"} choose={pear} />
      <Choice label="fig" chosen={picked === "fig"} choose={fig} />
      <Choice label="leek" chosen={picked === "leek"} choose={() => setPicked("leek")} />
    </div>
  );
}
