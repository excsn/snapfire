import type { Ctx } from "@snapfire/fsr";

export async function load({ services }: Ctx) {
  return { alerts: await services.fleet.listAlerts() };
}
