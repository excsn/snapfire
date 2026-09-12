import { island, Island, Link } from "@snapfire/fsr-client/react";

import type { TalkIdProps } from "@generated/client";
import { Feedback } from "@src/ui/Feedback";
import SaveTalk from "@src/ui/SaveTalk";

/** Below the abstract and not wanted at first paint, so it hydrates when it is scrolled to. */
const WhenSeen = island(Feedback, { when: "visible" });

export default function TalkPage({ talk, alongside, saved }: TalkIdProps) {
  return (
    <article className="page talk">
      <p className="when">
        {talk.starts}–{talk.ends} · {talk.room} · {talk.track} · {talk.level}
      </p>
      <h2>{talk.title}</h2>
      <p className="by">{talk.speaker}</p>
      <Island when="load">
        <SaveTalk id={talk.id} saved={saved} />
      </Island>
      <p className="abstract">{talk.abstract}</p>
      <section className="alongside">
        <h3>At the same time</h3>
        {alongside.length === 0 ? (
          <p className="quiet">Nothing else is on.</p>
        ) : (
          <ul>
            {alongside.map((other) => (
              <li key={other.id}>
                <Link href={`/talk/${other.id}`}>{other.title}</Link>
                <span className="room">{other.room}</span>
              </li>
            ))}
          </ul>
        )}
      </section>
      <div className="filler" />
      <WhenSeen title={talk.title} />
    </article>
  );
}
