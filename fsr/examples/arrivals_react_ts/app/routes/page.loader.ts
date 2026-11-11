import type { Ctx } from "@snapfire/fsr/host";

export async function load({ services }: Ctx) {
  const board = await services.board.getBoard();
  const now = Temporal.PlainTime.from(board.at);
  const coming = board.arrivals.filter((flight) => Temporal.PlainTime.compare(flight.expected, now) > 0);
  const next = coming.length === 0 ? null : { flight: coming[0].flight, minutes: now.until(coming[0].expected).total("minutes") };
  return { at: board.at, arrivals: board.arrivals, departures: board.departures, next, served: new Date().toTimeString() };
}

export const meta = ({ data }: { data: { at: string } }) => ({ title: `Arrivals · ${data.at}` });
