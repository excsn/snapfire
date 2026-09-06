import type { RootProps } from "@generated/client";
import { Link } from "@snapfire/fsr-client/react";

export default function HomePage({ cards }: RootProps) {
  return (
    <div className="page home">
      <h1>A site with no server</h1>
      <p className="lede">Every route below was rendered once, written to a file and served by a plain static host. There is no process answering requests.</p>
      <div className="cards">
        {cards.map((card) => (
          <article key={card.title} className="card">
            <h2>{card.title}</h2>
            <p>{card.body}</p>
          </article>
        ))}
      </div>
      <p>
        <Link href="/install">Start here</Link>
      </p>
    </div>
  );
}
