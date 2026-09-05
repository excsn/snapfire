import type { Ctx } from "@snapfire/fsr";

export async function load({ services }: Ctx) {
  const agents = await services.fleet.listAgents({ region: "all" });
  const busy = agents.filter((a) => a.queue_depth > 0n);
  const regions = ["eu", "us", "ap"].filter((r) => agents.some((a) => a.region === r));
  return { total: BigInt(agents.length), busy: BigInt(busy.length), regions };
}

export const meta = ({ data }: { data: { total: bigint } }) => ({ title: `${data.total} agents \u00b7 Ops console` });
