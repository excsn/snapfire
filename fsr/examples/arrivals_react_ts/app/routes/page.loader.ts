import type { Ctx } from "@snapfire/fsr";

export async function load({ services }: Ctx) {
  const board = await services.board.getBoard();
  return { at: board.at, arrivals: board.arrivals, departures: board.departures };
}

export const meta = ({ data }: { data: { at: string } }) => ({ title: `Arrivals · ${data.at}` });
