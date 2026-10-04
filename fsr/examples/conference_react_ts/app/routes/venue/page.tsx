import { Island } from "@snapfire/fsr-client/react";

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
      <p className="note">The time zone is the visitor's, which only a browser knows, so the clock renders in the browser. The rooms are a Vue island.</p>
    </section>
  );
}
