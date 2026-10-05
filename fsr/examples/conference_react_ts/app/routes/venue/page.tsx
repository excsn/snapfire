import { Island } from "@snapfire/fsr-client/react";

import AskDesk from "@src/ui/AskDesk";
import Floors from "@src/ui/Floors.vue";
import LocalClock from "@src/ui/LocalClock";
import type { VenueProps } from "@generated/client";

export default function VenuePage({ venue, talks }: VenueProps) {
  return (
    <section className="page venue-page">
      <h2>{venue}</h2>
      <LocalClock />
      <Island when="visible">
        <Floors talks={talks} />
      </Island>
      <Island when="load" define="@src/elements/talk-count.ts">
        <talk-count talks={talks.length}>{talks.length} talks</talk-count>
      </Island>
      <Island mode="server">
        <AskDesk desk="help" />
      </Island>
      <p className="note">The time zone is the visitor's, which only a browser knows, so the clock renders in the browser. The rooms are a Vue island, the talk count a custom element and the help desk a server island.</p>
    </section>
  );
}
