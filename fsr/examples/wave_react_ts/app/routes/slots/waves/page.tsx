import type { LayoutWavesProps } from "@generated/client";
import { Link } from "@snapfire/fsr-client/react";

/** The inbox. It marks the wave that is open by comparing the request's path with each wave's own, which is the one thing on this pane that a slot could not do before `ctx.path`. */
export default function Inbox({ waves, path }: LayoutWavesProps) {
  return (
    <ul className="wave-list">
      {waves.map((wave) => (
        <li key={wave.id}>
          <Link href={`/wave/${wave.id}`} className={path === `/wave/${wave.id}` ? "wave-card open" : "wave-card"}>
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
