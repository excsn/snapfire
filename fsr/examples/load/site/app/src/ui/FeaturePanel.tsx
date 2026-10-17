import type { Media } from "./types";

export default function FeaturePanel({ id, heading, body, media, points, reverse, cta, footnote }: { id: string; heading: string; body: string; media: Media; points: string[]; reverse: boolean; cta?: { label: string; href: string }; footnote?: string }) {
  return (
    <section id={id} className={reverse ? "panel panel-reverse" : "panel"}>
      <img className="panel-media" src={media.fallback} alt={media.alt} width={media.width} height={media.height} loading="lazy" />
      <div className="panel-copy">
        <h2>{heading}</h2>
        <p>{body}</p>
        {points.length > 0 && (
          <ul className="points">
            {points.map((point) => (
              <li key={point}>{point}</li>
            ))}
          </ul>
        )}
        {cta && (
          <a className="button button-secondary" href={cta.href}>
            {cta.label}
          </a>
        )}
        {footnote && <small className="footnote">{footnote}</small>}
      </div>
    </section>
  );
}
