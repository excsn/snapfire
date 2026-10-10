import type { Ctx } from "@snapfire/fsr";
import { t } from "@snapfire/fsr-client/std";

export async function load({ session, services }: Ctx) {
  const agents = await services.fleet.listAgents({ region: "all" });
  const watched = agents.filter((a) => session.watching[String(a.id)] === true);
  return { watched, density: session.density };
}

export const meta = () => ({ title: t("meta.settings") });
