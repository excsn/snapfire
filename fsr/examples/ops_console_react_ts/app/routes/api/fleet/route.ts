import type { Ctx, Failure } from "@snapfire/fsr/host";

export async function GET({ session, services }: Ctx<"/api/fleet">) {
  try {
    const alerts = await services.fleet.listAlerts();
    return { open: alerts.length, watching: Object.keys(session.watching).length };
  } catch (e) {
    return Response.json({ error: (e as Failure).message }, { status: 503, headers: { "retry-after": "30" } });
  }
}
