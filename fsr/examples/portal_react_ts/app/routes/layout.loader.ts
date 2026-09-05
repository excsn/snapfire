import type { Ctx } from "@snapfire/fsr";

export async function load({ identity, services }: Ctx) {
  const teams = await services.directory.listTeams();
  return { who: identity?.subject ?? "", teams: BigInt(teams.length) };
}

/** Seeded for every document the portal serves, a mounted site's included: the keys a site reads through the shell contract. */
export const store = ({ data }: { data: { who: string; teams: bigint } }) => ({
  "portal/who": data.who,
  "portal/teams": Number(data.teams),
});
