import { Island } from "@snapfire/fsr-client/react";
import { Embed } from "@snapfire/fsr/components";

import AskDesk from "@src/ui/AskDesk";
import Checklist from "@src/ui/Checklist";
import Floors from "@src/ui/Floors.vue";
import LocalClock from "@src/ui/LocalClock";
import type { VenueProps } from "@generated/client";
import walkMp4 from "@src/media/walk.mp4";
import walkWebm from "@src/media/walk.webm";
import walkCaptions from "@src/media/walk.vtt";
import walkPoster from "@src/media/walk-poster.png";

export default function VenuePage({ venue, talks }: VenueProps) {
  return (
    <section className="page venue-page">
      <h2>{venue}</h2>
      <LocalClock />
      <div className="venue-map" data-venue-map={venue} />
      <video className="venue-walk" controls preload="metadata" playsInline width={480} height={270} poster={walkPoster.src}>
        <source src={walkWebm} type="video/webm" />
        <source src={walkMp4} type="video/mp4" />
        <track kind="captions" src={walkCaptions} srcLang="en" label="English" default />
      </video>
      <section className="venue-breaks">
        <h3>Between the talks</h3>
        <Embed provider="youtube" id="f_WuRfuMXQw" title="Rick Astley - Never Gonna Give You Up" />
        <Embed provider="youtube" id="astISOttCQ0" title="Gummibär - The Gummy Bear Song" />
        <Embed provider="vimeo" id="76979871" title="The New Vimeo Player (You Know, For Videos)" />
      </section>
      <Island when="visible">
        <Floors talks={talks} />
      </Island>
      <Island when="load" define="@src/elements/talk-count.ts">
        <talk-count talks={talks.length}>{talks.length} talks</talk-count>
      </Island>
      <Island mode="server">
        <AskDesk desk="help" />
      </Island>
      <Island when="load">
        <Checklist />
      </Island>
      <label className="venue-note">
        A note for the day
        <textarea name="note" rows={2} />
      </label>
      <p className="note">The time zone is the visitor's, which only a browser knows, so the clock renders in the browser. The rooms are a Vue island, the talk count a custom element and the help desk a server island. The walk from the station is a plain video element whose files the page imports.</p>
    </section>
  );
}
