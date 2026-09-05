import type { Ctx } from "@snapfire/fsr";

export async function load({ query, session, services }: Ctx) {
  const region = query.region ?? "all";
  const agents = await services.fleet.listAgents({ region });
  return { region, agents, watching: Object.keys(session.watching) };
}

export const store = ({ data }: { data: { region: string } }) => ({ "fleet/region": data.region });
