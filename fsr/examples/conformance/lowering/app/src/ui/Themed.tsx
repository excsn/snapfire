import { useState } from "react";
import { Island } from "@snapfire/fsr-authoring/template";

import { Theme } from "@src/theme";
import Label from "@src/ui/Label";

export default function Themed({ mode }: { mode: string }) {
  const [current, setCurrent] = useState(mode);
  return (
    <Theme.Provider value={current}>
      <div className="themed">
        <Label name="inline" />
        <Island>
          <Label name="island" />
        </Island>
        <button className="flip" onClick={() => setCurrent(current === "dark" ? "light" : "dark")}>
          flip
        </button>
      </div>
    </Theme.Provider>
  );
}
