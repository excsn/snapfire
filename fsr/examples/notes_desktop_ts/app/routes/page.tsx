import type { RootProps } from "@generated/client";
import { Picker } from "../src/picker";

export default function Page({ notes, path, text, refused }: RootProps) {
  return (
    <main>
      <h1>Notes</h1>
      <Picker />
      <nav>
        {notes.map((name) => (
          <a key={name} href={`/?path=${encodeURIComponent(`notes/${name}`)}`}>{name}</a>
        ))}
      </nav>
      <h2>{path}</h2>
      {refused ? <p className="refused">{refused}</p> : <pre>{text}</pre>}
    </main>
  );
}
