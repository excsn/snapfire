import type { LayoutSponsorsProps } from "@generated/client";

export default function Sponsors({ sponsors }: LayoutSponsorsProps) {
  return (
    <div className="panel sponsors">
      <h2>With thanks to</h2>
      <ul>
        {sponsors.map((sponsor) => (
          <li key={sponsor.name}>
            <span className="name">{sponsor.name}</span>
            <span className="tier">{sponsor.tier}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}
