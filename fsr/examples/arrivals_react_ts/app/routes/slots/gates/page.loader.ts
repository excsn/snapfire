import type { Ctx } from "@snapfire/fsr";

export async function load({ services }: Ctx) {
  const changes = await services.board.listGateChanges();
  return { changes };
}
