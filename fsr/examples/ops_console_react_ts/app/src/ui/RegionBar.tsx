import { Link } from "@snapfire/fsr-client/react";

const REGIONS = ["all", "eu", "us", "ap"];

export function RegionBar({ region, shown }: { region: string; shown: bigint }) {
  return (
    <div className="region-bar">
      {REGIONS.map((r) => (
        <Link key={r} href={r === "all" ? "/agents" : `/agents?region=${r}`} className={r === region ? "chip chip-on" : "chip"}>
          {r}
        </Link>
      ))}
      <span className="region-count">{String(shown)} shown</span>
    </div>
  );
}
