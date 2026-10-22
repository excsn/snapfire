import type { Ctx, Failure } from "@snapfire/fsr";

export async function GET({ session, services }: Ctx<"/api/fleet">) {
  try {
    const alerts = await services.fleet.listAlerts();
    return { open: BigInt(alerts.length), watching: BigInt(Object.keys(session.watching).length) };
  } catch (e) {
    return Response.json({ error: (e as Failure).message }, { status: 503, headers: { "retry-after": "30" } });
  }
}
