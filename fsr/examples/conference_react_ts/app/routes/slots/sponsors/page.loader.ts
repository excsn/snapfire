import type { Ctx } from "@snapfire/fsr";

export async function load({ services }: Ctx) {
  const sponsors = await services.program.listSponsors();
  return { sponsors };
}
