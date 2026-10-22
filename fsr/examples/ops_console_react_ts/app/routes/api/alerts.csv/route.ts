import type { Ctx, Failure } from "@snapfire/fsr";

export async function GET({ services }: Ctx<"/api/alerts.csv">) {
  let body = "id,agent,level,text\n";
  let status = 200;
  try {
    const alerts = await services.fleet.listAlerts();
    for (const a of alerts) {
      body += `${a.id},${a.agent_id},${a.level},"${a.text}"\n`;
    }
  } catch (e) {
    status = 503;
    body = `The fleet did not answer: ${(e as Failure).message}\n`;
  } finally {
    console.info(`alerts.csv answered ${status}`);
  }
  const type = status === 200 ? "text/csv; charset=utf-8" : "text/plain; charset=utf-8";
  return new Response(body, { status, headers: { "content-type": type, "content-disposition": "attachment; filename=alerts.csv" } });
}
