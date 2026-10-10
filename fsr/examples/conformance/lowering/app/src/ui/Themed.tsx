import { useState } from "react";
import { Island } from "@snapfire/fsr-authoring/template";

import { Theme } from "@src/theme";
import Label from "@src/ui/Label";
import Tag from "@src/ui/Tag";

export default function Themed({ mode }: { mode: string }) {
  const [current, setCurrent] = useState(mode);
  return (
    <Theme value={current}>
      <div className="themed">
        <Label name="inline" />
        <Tag loud={true} />
        <Island>
          <Label name="island" />
        </Island>
        <button className="flip" onClick={() => setCurrent(current === "dark" ? "light" : "dark")}>
          flip
        </button>
      </div>
    </Theme>
  );
}
