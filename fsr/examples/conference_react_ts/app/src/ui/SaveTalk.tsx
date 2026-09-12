import { useState } from "react";
import { get, optimistic } from "@snapfire/fsr-client";

import { actions } from "@generated/client";
import { savedCount } from "@src/store";

export default function SaveTalk({ id, saved }: { id: string; saved: boolean }) {
  const [held, setHeld] = useState(saved);
  const [why, setWhy] = useState("");

  async function toggle(): Promise<void> {
    const step = held ? -1 : 1;
    try {
      await optimistic(savedCount, (get(savedCount) ?? 0) + step, () =>
        held ? actions.talk.$id.drop({ talk_id: id }) : actions.talk.$id.save({ talk_id: id }),
      );
      setHeld(!held);
      setWhy("");
    } catch (e) {
      setWhy(e instanceof Error ? e.message : "that did not take");
    }
  }

  return (
    <p className="save">
      <button className={held ? "btn btn-on" : "btn"} onClick={() => void toggle()}>
        {held ? "On your schedule" : "Add to my schedule"}
      </button>
      {why === "" ? null : <span className="why">{why}</span>}
    </p>
  );
}
