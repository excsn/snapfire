import type { Ctx } from "@snapfire/fsr";

export async function load({ services }: Ctx) {
  const notices = await services.program.listAnnouncements();
  return { notices };
}
