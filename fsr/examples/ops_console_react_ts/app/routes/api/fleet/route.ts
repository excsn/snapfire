import type { Ctx } from "@snapfire/fsr";

export async function GET({ session, services }: Ctx<"/api/fleet">) {
  const alerts = await services.fleet.listAlerts();
  return { open: BigInt(alerts.length), watching: BigInt(Object.keys(session.watching).length) };
}
