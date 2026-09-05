import type { Ctx } from "@snapfire/fsr";

export async function load({ identity, services }: Ctx) {
  const agents = await services.fleet.listAgents({ region: "all" });
  return { subject: identity?.subject ?? "", role: String(identity?.claims.role ?? "member"), agents: BigInt(agents.length) };
}

export const meta = () => ({ title: "Account · Ops console" });
