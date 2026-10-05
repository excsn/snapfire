import { useState } from "react";
import { action } from "@snapfire/fsr-client";
import { useLocale } from "@snapfire/fsr-client/react";

const touch = action("ctx.$id.touch");

export default function CtxStep() {
  const locale = useLocale();
  const [steps, setSteps] = useState(0);
  return (
    <div className="ctx-step">
      <p className="step-locale">{locale}</p>
      <button
        className="step"
        onClick={() => {
          setSteps(steps + 1);
          void touch({ note: "from the server" });
        }}
      >
        step
      </button>
      <p className="steps">{steps}</p>
    </div>
  );
}
