import type { Ctx } from "@snapfire/fsr";

export async function load({ services }: Ctx) {
  const waves = await services.waves.listWaves();
  return { waves };
}
