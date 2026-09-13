import { Island } from "@snapfire/fsr-client/react";

import type { WaveIdProps } from "@generated/client";
import Blip from "@src/ui/Blip";
import Gadget from "@src/ui/Gadget";
import Presence from "@src/ui/Presence";
import Under from "@src/ui/Under";

/** The wave itself is rendered here, on the server: every blip with its replies inside it and each reply to one part of a blip under that part. `Blip` walks the tree the service built. */
export default function WavePage({ wave, me }: WaveIdProps) {
  return (
    <article className="wave">
      <header className="wave-head">
        <h1>{wave.title}</h1>
        <Island when="load">
          <Presence wave={wave.id} participants={wave.participants} />
        </Island>
      </header>

      <Island mode="server">
        <Gadget wave={wave.id} cells={wave.game.cells} turn={wave.game.turn} won={wave.game.won} />
      </Island>

      <ol className="blips">
        {wave.blips.map((blip) => (
          <Blip key={blip.id} wave={wave.id} blip={blip} me={me} />
        ))}
      </ol>

      <Island when="load">
        <Under wave={wave.id} parent="" me={me} open />
      </Island>
    </article>
  );
}
