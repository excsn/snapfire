import { Island } from "@snapfire/fsr-client/react";

import type { WaveIdProps } from "@generated/client";
import Body from "@src/ui/Body";
import Presence from "@src/ui/Presence";
import Under from "@src/ui/Under";

/** The wave itself is rendered here, on the server: every blip, in reading order, at the depth the service gave it. A blip's text is an island because a blip is a document: anyone on the wave may rewrite it, and everyone else watches while they do. */
export default function WavePage({ wave, me }: WaveIdProps) {
  return (
    <article className="wave">
      <header className="wave-head">
        <h1>{wave.title}</h1>
        <Island when="load">
          <Presence wave={wave.id} participants={wave.participants} />
        </Island>
      </header>

      <ol className="blips">
        {wave.blips.map((blip) => (
          <li key={blip.id} style={{ marginLeft: `${Number(blip.depth) * 1.5}rem` }}>
            <div className={blip.who === me ? "blip mine" : "blip"}>
              <span className="who">{blip.who}</span>
              <span className="at">{blip.at}</span>
              <Island when="load">
                <Body wave={wave.id} blip={blip.id} text={blip.body} edited={blip.edited} editors={blip.editors} me={me} />
              </Island>
            </div>
            <Island when="load">
              <Under wave={wave.id} parent={blip.id} me={me} />
            </Island>
          </li>
        ))}
      </ol>

      <Island when="load">
        <Under wave={wave.id} parent="" me={me} open />
      </Island>
    </article>
  );
}
