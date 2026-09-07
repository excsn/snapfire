import type { Ctx } from "@snapfire/fsr";

export async function load({ query, session, services, path }: Ctx) {
  const waves = await services.waves.listWaves({ view: query.view, who: session.name });
  return { waves, path };
}
