import type { RootProps } from "@generated/client";
import { Link } from "@snapfire/fsr-client/react";

export default function IndexPage({ teams, visits }: RootProps) {
  return (
    <div>
      <h1>Teams</h1>
      <p className="mb-5 text-ink-soft">Each team owns the site under its path and deploys it on its own schedule. This is your visit number {Number(visits)} in this session.</p>
      <ul className="grid gap-3">
        {teams.map((team) => (
          <li key={team.name} className="flex items-baseline gap-4 rounded-[10px] border border-line bg-card px-[18px] py-3.5">
            <Link href={team.site}>{team.name}</Link>
            <span className="text-[13px] text-ink-soft">lead {team.lead}</span>
            <code className="ml-auto">{team.site}</code>
          </li>
        ))}
      </ul>
    </div>
  );
}
