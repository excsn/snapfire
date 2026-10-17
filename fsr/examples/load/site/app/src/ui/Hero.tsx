import type { Media } from "./types";

export default function Hero({ id, heading, body, media, cta }: { id: string; heading: string; body: string; media: Media; cta: { label: string; href: string } }) {
  return (
    <section id={id} className="hero">
      <div className="hero-copy">
        <h1>{heading}</h1>
        <p>{body}</p>
        <a className="button" href={cta.href}>
          {cta.label}
        </a>
      </div>
      <picture className="hero-media">
        {media.sources.map((source) => (
          <source key={source.width} srcSet={source.url} media={`(min-width: ${source.width}px)`} type={source.type} />
        ))}
        <img src={media.fallback} alt={media.alt} width={media.width} height={media.height} loading="eager" />
      </picture>
    </section>
  );
}
