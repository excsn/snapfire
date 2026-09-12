import { Link } from "@snapfire/fsr-client/react";

import type { RootProps } from "@generated/client";

export default function SchedulePage({ track, tracks, talks, saved }: RootProps) {
  return (
    <section className="page schedule">
      <h2>The day</h2>
      <nav className="tracks">
        <Link href="/" className={track === "all" ? "chip chip-on" : "chip"}>
          Everything
        </Link>
        {tracks.map((name) => (
          <Link key={name} href={`/?track=${encodeURIComponent(name)}`} className={track === name ? "chip chip-on" : "chip"}>
            {name}
          </Link>
        ))}
      </nav>
      <ol className="talks">
        {talks.map((talk) => (
          <li key={talk.id} className="talk-row">
            <span className="at">
              {talk.starts}–{talk.ends}
            </span>
            <span className="what">
              <Link href={`/talk/${talk.id}`} className="talk-title">
                {talk.title}
              </Link>
              <span className="by">{talk.speaker}</span>
            </span>
            <span className={`track track-${talk.track.toLowerCase()}`}>{talk.track}</span>
            <span className="room">{talk.room}</span>
            {saved.includes(talk.id) ? <span className="kept">on your schedule</span> : <span className="kept kept-off" />}
          </li>
        ))}
      </ol>
      {talks.length === 0 ? <p className="quiet">Nothing on that track.</p> : null}
    </section>
  );
}
