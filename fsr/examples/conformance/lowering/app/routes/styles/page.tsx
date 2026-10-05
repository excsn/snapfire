import Bars from "@src/ui/Bars";

export default function Styles({ rows }: { rows: { name: string; ms: number }[] }) {
  return (
    <section className="styles">
      <p>Each bar's width is a <code>style</code> attribute the server renders, admitted by the policy's <code>style-src-attr</code>.</p>
      <Bars rows={rows} />
    </section>
  );
}
