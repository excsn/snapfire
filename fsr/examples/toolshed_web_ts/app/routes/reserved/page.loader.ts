import type { Ctx } from "@snapfire/fsr";

export async function load({ session, services }: Ctx) {
  const tools = await services.shed.listTools();
  const reserved = tools.filter((t) => (session.reserved[t.id] ?? 0) > 0).map((t) => ({ ...t, days: session.reserved[t.id] }));
  const deposit = reserved.reduce((sum: bigint, t) => sum + t.deposit, 0n);
  return { reserved, deposit };
}

export const meta = () => ({ title: "Reserved · The Shed" });
