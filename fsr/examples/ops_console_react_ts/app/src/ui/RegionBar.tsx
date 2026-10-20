import { Link } from "@snapfire/fsr/components";
import { t } from "@snapfire/fsr-client/std";

const REGIONS = ["all", "eu", "us", "ap"];

export function RegionBar({ region, shown }: { region: string; shown: bigint }) {
  return (
    <div className="region-bar">
      {REGIONS.map((r) => (
        <Link key={r} href={r === "all" ? "/agents" : `/agents?region=${r}`} className={r === region ? "chip chip-on" : "chip"}>
          {r === "all" ? t("agents.all") : r}
        </Link>
      ))}
      <span className="region-count">{t("agents.shown", { count: shown })}</span>
    </div>
  );
}
