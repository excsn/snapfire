import type { IndexProps } from "@generated/client";
import { Link } from "@snapfire/fsr-client/react";

export default function IndexPage({ teams, visits }: IndexProps) {
  return (
    <div className="page home">
      <h1>Teams</h1>
      <p className="lede">Each team owns the site under its path and deploys it on its own schedule. This is your visit number {Number(visits)} in this session.</p>
      <ul className="teams">
        {teams.map((team) => (
          <li key={team.name} className="team">
            <Link href={team.site}>{team.name}</Link>
            <span className="lead">lead {team.lead}</span>
            <code>{team.site}</code>
          </li>
        ))}
      </ul>
    </div>
  );
}
