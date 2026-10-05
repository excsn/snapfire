import { useState } from "react";

export default function Vowels({ words }: { words: string[] }) {
  const [only, setOnly] = useState(false);
  const shown = only ? words.filter((word) => /^[aeiou]/i.test(word)) : words;
  return (
    <div className="vowels">
      <p className="shown">{shown.join(" ")}</p>
      <button className="toggle" onClick={() => setOnly(!only)}>
        {only ? "all" : "vowels"}
      </button>
    </div>
  );
}
