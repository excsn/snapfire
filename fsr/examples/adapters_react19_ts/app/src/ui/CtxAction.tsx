import { useState } from "react";
import { useLocale } from "@snapfire/fsr-client/react";

import { actions } from "@generated/client";

export default function CtxAction() {
  const locale = useLocale();
  const [answer, setAnswer] = useState("");
  async function touch(): Promise<void> {
    const got = await actions.ctx.$id.touch({ note: "from react" });
    setAnswer(JSON.stringify(got));
  }
  return (
    <div className="ctx-action">
      <p className="island-locale">{locale}</p>
      <button className="touch" onClick={() => void touch()}>
        touch
      </button>
      <pre className="answer">{answer}</pre>
    </div>
  );
}
