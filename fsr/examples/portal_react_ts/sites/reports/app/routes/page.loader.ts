import type { Ctx } from "@snapfire/fsr";

export async function load({ services }: Ctx) {
  const reports = await services.metrics.listReports();
  return { reports };
}

export const meta = () => ({ title: "Reports" });
