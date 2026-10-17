import Accordion from "@src/ui/Accordion";

export default function Regions({ heading, answers }: { heading: string; answers: { id: string; q: string; a: string }[] }) {
  return (
    <Accordion
      label={
        <>
          Questions, <em>{heading}</em>
        </>
      }
      items={answers.map((d) => ({ id: d.id, q: d.q, body: <p className="answer">{d.a}</p> }))}
    />
  );
}
