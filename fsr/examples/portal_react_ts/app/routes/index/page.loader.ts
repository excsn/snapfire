import type { Ctx } from "@snapfire/fsr";

export async function load({ services, session }: Ctx) {
  session.visits = session.visits + 1;
  const teams = await services.directory.listTeams();
  return { teams, visits: session.visits };
}

export const meta = () => ({ title: "Acme portal" });
