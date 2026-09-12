import { Link } from "@snapfire/fsr-client/react";

import type { SavedProps } from "@generated/client";

export default function SavedPage({ mine }: SavedProps) {
  return (
    <section className="page saved-page">
      <h2>My schedule</h2>
      {mine.length === 0 ? (
        <p className="quiet">
          Nothing kept yet. Open <Link href="/talk/1">a talk</Link> and add it.
        </p>
      ) : null}
      <ol className="talks">
        {mine.map((talk) => (
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
            <span className="room">{talk.room}</span>
          </li>
        ))}
      </ol>
      <p className="note">This page reads the session. Nothing about it is cached.</p>
    </section>
  );
}
