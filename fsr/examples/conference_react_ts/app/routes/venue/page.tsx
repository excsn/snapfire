import { Island } from "@snapfire/fsr/client/react";
import { Embed, Link } from "@snapfire/fsr/components";

import AskDesk from "@src/ui/AskDesk";
import Checklist from "@src/ui/Checklist";
import Floors from "@src/ui/Floors.vue";
import LocalClock from "@src/ui/LocalClock";
import type { VenueProps } from "@generated/client";
import walkMp4 from "@src/media/walk.mp4";
import walkWebm from "@src/media/walk.webm";
import walkCaptions from "@src/media/walk.vtt";
import walkPoster from "@src/media/walk-poster.png";

export default function VenuePage({ venue, track, tracks, talks }: VenueProps) {
  return (
    <section className="page venue-page">
      <h2>{venue}</h2>
      <LocalClock />
      <div className="venue-map" data-venue-map={venue} />
      <div className="views">
        <input type="radio" name="view" id="view-walk" className="view-pick" defaultChecked />
        <input type="radio" name="view" id="view-notices" className="view-pick" />
        <div className="view-tabs">
          <label htmlFor="view-walk">Getting there</label>
          <label htmlFor="view-notices">Notices</label>
        </div>
        <div className="view-panels">
          <div className="view-panel">
            <video className="venue-walk" controls preload="metadata" playsInline width={480} height={270} poster={walkPoster.src}>
              <source src={walkWebm} type="video/webm" />
              <source src={walkMp4} type="video/mp4" />
              <track kind="captions" src={walkCaptions} srcLang="en" label="English" default />
            </video>
          </div>
          <div className="view-panel">
            <div className="venue-notices" hx-get="/venue/notices?__fragment" hx-trigger="intersect once" hx-swap="outerHTML">
              Loading the notices
            </div>
          </div>
        </div>
      </div>
      <section className="venue-breaks">
        <h3>Between the talks</h3>
        <Embed provider="youtube" id="f_WuRfuMXQw" title="Rick Astley - Never Gonna Give You Up" />
        <Embed provider="youtube" id="astISOttCQ0" title="Gummibär - The Gummy Bear Song" />
        <Embed provider="vimeo" id="76979871" title="The New Vimeo Player (You Know, For Videos)" />
      </section>
      <Island when="visible">
        <Floors talks={talks} />
      </Island>
      <nav className="tracks venue-tracks">
        <Link href="/venue" className={track === "all" ? "chip chip-on" : "chip"}>
          All tracks
        </Link>
        {tracks.map((name) => (
          <Link key={name} href={`/venue?track=${encodeURIComponent(name)}`} className={track === name ? "chip chip-on" : "chip"}>
            {name}
          </Link>
        ))}
      </nav>
      <Island when="load" define="@src/elements/talk-count.ts">
        <talk-count talks={talks.length} track={track === "all" ? undefined : track}>
          {talks.length} talks
        </talk-count>
      </Island>
      <Island when="load" define="@src/elements/talk-time.ts">
        <div className="venue-times">
          <p className="venue-first">
            First talk <talk-time starts={talks[0]?.starts}>{talks[0]?.starts}</talk-time>
          </p>
          <ul>
            {talks.map((talk) => (
              <li key={talk.id}>
                <talk-time starts={talk.starts}>{talk.starts}</talk-time> {talk.title}
              </li>
            ))}
          </ul>
        </div>
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
      <p className="note">The time zone is the visitor&apos;s, which only a browser knows, so the clock renders in the browser. The rooms are a Vue island, the talk count a custom element and the help desk a server island. The walk from the station is a plain video element whose files the page imports. The notices are a page htmx fetches once their tab is open.</p>
    </section>
  );
}
