import type { Ctx } from "@snapfire/fsr";

export async function load({ services }: Ctx) {
  const notes = await services.kitchen.listNotes();
  return { notes };
}
