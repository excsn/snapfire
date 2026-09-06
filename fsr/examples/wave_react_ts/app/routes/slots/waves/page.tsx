import type { LayoutWavesProps } from "@generated/client";
import { Link } from "@snapfire/fsr-client/react";

export default function Inbox({ waves }: LayoutWavesProps) {
  return (
    <ul className="wave-list">
      {waves.map((wave) => (
        <li key={wave.id}>
          <Link href={`/wave/${wave.id}`} className="wave-card">
            <span className="wave-title">{wave.title}</span>
            <span className="wave-when">{wave.last}</span>
            <span className="wave-people">{wave.participants.join(", ")}</span>
            <span className="wave-blips">{wave.blips}</span>
          </Link>
        </li>
      ))}
    </ul>
  );
}
