import type { Ctx } from "@snapfire/fsr";

export async function load({ params, services }: Ctx<"/{id}">) {
  const report = await services.metrics.getReport({ id: BigInt(params.id) });
  return { report };
}

export const meta = ({ data }: { data: { report: { name: string } } }) => ({ title: `${data.report.name} · Reports` });

export async function paths({ services }: Ctx) {
  const reports = await services.metrics.listReports();
  return reports.map((report) => ({ id: `${report.id}` }));
}
