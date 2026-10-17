import Hero from "@src/ui/Hero";
import FeaturePanel from "@src/ui/FeaturePanel";
import RichText from "@src/ui/RichText";

export default function Lowered({ hero, panels, terms }: any) {
  return (
    <>
      <Hero id="hero" heading={hero.heading} body={hero.body} media={hero.media} cta={hero.cta} />
      {panels.map((panel: any) => (
        <FeaturePanel key={panel.id} id={panel.id} heading={panel.heading} body={panel.body} media={panel.media} points={panel.points} reverse={panel.reverse} cta={panel.cta} footnote={panel.footnote} />
      ))}
      <section id="terms">
        <RichText nodes={terms} />
      </section>
    </>
  );
}
