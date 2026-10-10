import type { Ctx } from "@snapfire/fsr";

export async function load({ session, services }: Ctx) {
  const alerts = await services.fleet.listAlerts();
  return { open: BigInt(alerts.length), watching: BigInt(Object.keys(session.watching).length), density: session.density };
}

export const store = ({ data }: { data: { open: bigint; watching: bigint; density: string } }) => ({
  "alerts/open": Number(data.open),
  "fleet/watching": Number(data.watching),
  "fleet/region": "all",
  "ui/density": data.density,
});
