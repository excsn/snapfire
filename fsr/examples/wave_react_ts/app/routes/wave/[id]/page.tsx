import { Island } from "@snapfire/fsr-client/react";

import type { WaveIdProps } from "@generated/client";
import Blip from "@src/ui/Blip";
import Playback from "@src/ui/Playback";
import Presence from "@src/ui/Presence";
import Under from "@src/ui/Under";

/** The wave itself is rendered here, on the server: every blip with its replies inside it and each reply to one part of a blip under that part. `Blip` walks the tree the service built. Under `?at=` it is the wave after that step of its log, which nobody can write on. */
export default function WavePage({ wave, me }: WaveIdProps) {
  return (
    <article className={wave.live ? "wave" : "wave replaying"}>
      <header className="wave-head">
        <h1>{wave.title}</h1>
        <Island when="load">
          <Presence wave={wave.id} participants={wave.participants} />
        </Island>
      </header>

      <nav className="wave-nav" aria-label="playback">
        <Island when="load">
          <Playback wave={wave.id} step={wave.step} steps={wave.steps} live={wave.live} />
        </Island>
      </nav>

      <div className="transcript">
        <ol className="blips">
          {wave.blips.map((blip) => (
            <Blip key={blip.id} wave={wave.id} blip={blip} me={me} live={wave.live} />
          ))}
        </ol>

        {wave.live ? (
          <Island when="load">
            <Under wave={wave.id} parent="" me={me} open />
          </Island>
        ) : null}
      </div>
    </article>
  );
}
