import { Link } from "@snapfire/fsr/components";

import type { RootProps } from "@generated/client";
import Guard from "@src/ui/Guard";
import Opening from "@src/ui/Opening";

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
      <div className="views">
        <input type="radio" name="view" id="view-day" className="view-pick" defaultChecked />
        <input type="radio" name="view" id="view-rooms" className="view-pick" />
        <div className="view-tabs">
          <label htmlFor="view-day">By time</label>
          <label htmlFor="view-rooms">By room</label>
        </div>
        <div className="view-panels">
          <div className="view-panel">
            <Guard>
              <ol className="talks">
                {talks.map((talk) => (
                  <li key={talk.id} className="talk-row">
                    <span className="at">
                      {talk.starts}–{talk.ends}
                    </span>
                    <span className="what">
                      <Link href={`/talk/${talk.id}`} className="talk-title">
                        {talk.title}
                        <Opening />
                      </Link>
                      <span className="by">{talk.speaker}</span>
                    </span>
                    <span className={`track track-${talk.track.toLowerCase()}`}>{talk.track}</span>
                    <span className="room">{talk.room}</span>
                    {saved.includes(talk.id) ? <span className="kept">on your schedule</span> : <span className="kept kept-off" />}
                  </li>
                ))}
              </ol>
            </Guard>
          </div>
          <div className="view-panel">
            <ul className="rooms">
              {Array.from(new Set(talks.map((talk) => talk.room))).map((room) => (
                <li key={room}>
                  <b>{room}</b> {talks.filter((talk) => talk.room === room).length} on the day
                </li>
              ))}
            </ul>
          </div>
        </div>
      </div>
      {talks.length === 0 ? <p className="quiet">Nothing on that track.</p> : null}
    </section>
  );
}
