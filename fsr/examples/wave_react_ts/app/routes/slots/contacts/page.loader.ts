import type { Ctx } from "@snapfire/fsr";

export async function load({ services }: Ctx) {
  const people = await services.waves.listPeople();
  return { people };
}
