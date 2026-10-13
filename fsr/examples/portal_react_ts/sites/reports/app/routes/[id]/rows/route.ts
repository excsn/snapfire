import type { Ctx } from "@snapfire/fsr";

export async function GET({ params, services }: Ctx<"/{id}/rows">) {
  const report = await services.metrics.getReport({ id: BigInt(params.id) });
  return { name: report.name, rows: report.rows };
}
