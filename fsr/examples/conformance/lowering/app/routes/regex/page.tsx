import { Island } from "@snapfire/fsr-authoring/template";

import Vowels from "@src/ui/Vowels";

export default function Regex({ entries, term, years }: { entries: { title: string; slug: string }[]; term: string; years: string[] }) {
  return (
    <section className="regex">
      <ul className="entries">
        {entries.map(({ title, slug }) => (
          <li key={slug}>
            <a href={`/regex/${slug}`}>{title.replace(/\b(\w)(\w*)/g, (_, first: string, rest: string) => first.toUpperCase() + rest)}</a>
          </li>
        ))}
      </ul>
      <p className="hits">{entries.filter(({ title }) => title.toLowerCase().includes(term)).length} with {term}</p>
      <p className="years">{years.join(" → ")}</p>
      <p className="words">{entries[1].title.split(/,?\s+/).length} words</p>
      <Island>
        <Vowels words={entries[1].title.split(/[\s,]+/)} />
      </Island>
    </section>
  );
}
