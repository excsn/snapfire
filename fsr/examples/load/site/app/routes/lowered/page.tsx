import Hero from "@src/ui/Hero";
import FeaturePanel from "@src/ui/FeaturePanel";
import RichText from "@src/ui/RichText";
import type { Media, RichNode } from "@src/ui/types";

type Cta = { label: string; href: string };
type Panel = { id: string; heading: string; body: string; media: Media; points: string[]; reverse: boolean; cta?: Cta; footnote?: string };

export default function Lowered({ hero, panels, terms }: { hero: { heading: string; body: string; media: Media; cta: Cta }; panels: Panel[]; terms: RichNode[] }) {
  return (
    <>
      <Hero id="hero" heading={hero.heading} body={hero.body} media={hero.media} cta={hero.cta} />
      {panels.map((panel) => (
        <FeaturePanel key={panel.id} id={panel.id} heading={panel.heading} body={panel.body} media={panel.media} points={panel.points} reverse={panel.reverse} cta={panel.cta} footnote={panel.footnote} />
      ))}
      <section id="terms">
        <RichText nodes={terms} />
      </section>
    </>
  );
}
