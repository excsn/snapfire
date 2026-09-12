import type { Ctx } from "@snapfire/fsr";

export async function load({ session, services }: Ctx) {
  const talks = await services.program.listTalks();
  const held = session.saved;
  const mine = talks.filter((t) => held[t.id] ?? false);
  return { mine };
}

export const meta = ({ data }: { data: { mine: unknown[] } }) => ({
  title: `My schedule · ${data.mine.length} ${data.mine.length === 1 ? "talk" : "talks"}`,
});
