import { useEffect, useState } from "react";
import { navigate } from "@snapfire/fsr-client";

import { follow, toEnd } from "@src/ui/follow";

/** Playback of the wave's log, where a step is one kept blip, one amend, one move or one vote. On the wave as it stands it is one button, which opens playback at the last step, where the wave stands. Playback is this page under `?at=`, replayed and rendered by the server, so every move of the scrubber is a navigation that replaces its history entry and leaves the window where it is. The query is all that changes, so this island and its state survive it. The last step is still playback; only the exit button goes back to the wave as it stands. Playing steps on once a beat until the last step. Arriving at a wave as it stands brings the end of its transcript into view at once. The last step brings the end of the transcript into view, since the wave as it stands ends there. Every other step brings what it changed into view when it is out of view. */
export default function Playback({ wave, step, steps, live }: { wave: string; step: number; steps: number; live: boolean }) {
  const [playing, setPlaying] = useState(false);
  const [shown, setShown] = useState(step);
  useEffect(() => setShown(step), [step]);
  useEffect(() => {
    if (live) toEnd(true);
  }, [wave]);
  useEffect(() => {
    if (!playing || live || shown !== step) return undefined;
    const beat = setTimeout(() => go(step + 1), 800);
    return () => clearTimeout(beat);
  }, [playing, live, step, shown]);
  useEffect(() => {
    if (live) return;
    if (step === steps) {
      toEnd();
      return;
    }
    const lit = document.querySelector(".blip.lit, .gadget.lit");
    if (lit) follow(lit);
  }, [live, step, steps]);

  function visit(at: number | null): void {
    const url = new URL(window.location.href);
    if (at === null) url.searchParams.delete("at");
    else url.searchParams.set("at", String(at));
    void navigate(`${url.pathname}${url.search}`, true, { replace: true, scroll: false });
  }

  function go(to: number): void {
    const at = Math.max(0, Math.min(to, steps));
    if (at >= steps) setPlaying(false);
    setShown(at);
    visit(at);
  }

  function toggle(): void {
    if (!playing && shown >= steps) go(0);
    setPlaying(!playing);
  }

  if (live) {
    return (
      <button className="open-playback" onClick={() => visit(steps)} disabled={steps === 0}>
        Playback
      </button>
    );
  }

  return (
    <div className="playback">
      <div className="bar-head">
        <span className="count">{steps === 1 ? "1 change" : `${steps} changes`}</span>
        <button className="exit" title="leave playback" aria-label="leave playback" onClick={() => visit(null)}>
          ✕
        </button>
      </div>
      <button className="step" title="the start" onClick={() => go(0)} disabled={shown === 0}>
        ⏮
      </button>
      <button className="step" title="a step back" onClick={() => go(shown - 1)} disabled={shown === 0}>
        ◀
      </button>
      <button className="play" onClick={toggle} disabled={steps === 0}>
        {playing ? "Pause" : "Play"}
      </button>
      <input type="range" aria-label="step" min={0} max={steps} value={shown} onChange={(e) => go(Number(e.target.value))} />
      <button className="step" title="a step on" onClick={() => go(shown + 1)} disabled={shown >= steps}>
        ▶
      </button>
    </div>
  );
}
