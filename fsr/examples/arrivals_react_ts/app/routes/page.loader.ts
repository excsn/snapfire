import type { Ctx } from "@snapfire/fsr";

export async function load({ services }: Ctx) {
  const arrivals = await services.board.listArrivals();
  return { arrivals };
}

export const meta = () => ({ title: "Arrivals" });
