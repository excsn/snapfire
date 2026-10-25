import type { Ctx, Failure } from "@snapfire/fsr/host";

export async function load({ services }: Ctx) {
  try {
    return { alerts: await services.fleet.listAlerts(), down: "" };
  } catch (e) {
    return { alerts: [], down: (e as Failure).message };
  }
}
