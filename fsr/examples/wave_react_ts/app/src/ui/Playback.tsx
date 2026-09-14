import { useEffect, useState } from "react";
import { navigate } from "@snapfire/fsr-client";

import type { Change } from "@generated/client";

/** Brings `el` to the middle of what shows it when it is out of view, smoothly unless the reader asks for less motion. What shows it is the part of the transcript inside the window: the transcript scrolls on its own where the panes sit side by side and the window scrolls where they stack. */
function follow(el: Element): void {
  const box = el.getBoundingClientRect();
  const pane = el.closest(".blips")?.getBoundingClientRect();
  const top = Math.max(pane?.top ?? 0, 0);
  const bottom = Math.min(pane?.bottom ?? window.innerHeight, window.innerHeight);
  if (box.top >= top && box.bottom <= bottom) return;
  const still = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  el.scrollIntoView({ behavior: still ? "auto" : "smooth", block: "center" });
}

/** The scrubber over the wave's log, where a step is one kept blip, one amend, one move or one vote. The wave after a step is this page under `?at=`, replayed and rendered by the server, so moving the scrubber is a navigation that replaces its history entry and leaves the window where it is. The query is all that changes, so this island and its state survive it. The end of the log is the wave as it stands. Playing steps on once a beat until it gets there. Every step brings what it changed into view when it is out of view, the last one included once the wave is live again. */
export default function Playback({ step, steps, live, change }: { step: number; steps: number; live: boolean; change: Change }) {
  const [playing, setPlaying] = useState(false);
  const [finishing, setFinishing] = useState(false);
  const [shown, setShown] = useState(step);
  useEffect(() => setShown(step), [step]);
  useEffect(() => {
    if (!playing || live) return undefined;
    const beat = setTimeout(() => go(step + 1), 800);
    return () => clearTimeout(beat);
  }, [playing, live, step]);
  useEffect(() => {
    if (live) return;
    const lit = document.querySelector(".blip.lit, .gadget.lit");
    if (lit) follow(lit);
  }, [live, step]);
  useEffect(() => {
    if (!finishing || !live) return;
    setFinishing(false);
    const last = document.getElementById(`blip-${change.blip}`);
    if (last) follow(last);
  }, [finishing, live, change.blip]);

  function go(to: number): void {
    const at = Math.max(0, Math.min(to, steps));
    if (at >= steps) {
      if (!live) setFinishing(true);
      setPlaying(false);
    }
    setShown(at);
    const url = new URL(window.location.href);
    if (at >= steps) url.searchParams.delete("at");
    else url.searchParams.set("at", String(at));
    void navigate(`${url.pathname}${url.search}`, true, { replace: true, scroll: false });
  }

  function toggle(): void {
    setPlaying(!playing);
    if (!playing && live) go(0);
  }

  const did = change.kind === "kept" ? "wrote a blip" : change.kind === "amended" ? "rewrote a blip" : change.kind === "played" ? "moved" : change.kind === "voted" ? "voted" : "cleared a board";

  return (
    <div className={live ? "playback" : "playback on"}>
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
      <button className="step" title="a step on" onClick={() => go(shown + 1)} disabled={live}>
        ▶
      </button>
      <button className="to-live" onClick={() => go(steps)} disabled={live}>
        Live
      </button>
      <span className="said">{live ? `${steps} changes` : change.kind === "" ? `0 of ${steps}` : `${step} of ${steps}: ${change.who} ${did} at ${change.at}`}</span>
    </div>
  );
}
