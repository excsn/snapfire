import type { Ctx } from "@snapfire/fsr/host";
import { og } from "@generated/head";

export async function load({ services }: Ctx) {
  const reports = await services.metrics.listReports();
  return { reports };
}

export const meta = () => ({ title: "Reports", head: [og("type", "website")] });
