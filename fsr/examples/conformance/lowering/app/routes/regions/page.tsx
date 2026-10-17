import Accordion from "@src/ui/Accordion";

export default function Regions() {
  const answers = [
    { id: "why", q: "Why?", a: "Because" },
    { id: "how", q: "How?", a: "Like so" },
  ];
  return (
    <Accordion
      label={
        <>
          Questions, <em>answered</em>
        </>
      }
      items={answers.map((d) => ({ id: d.id, q: d.q, body: <p className="answer">{d.a}</p> }))}
    />
  );
}
