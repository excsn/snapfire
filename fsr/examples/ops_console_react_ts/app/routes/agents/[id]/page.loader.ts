import type { Ctx } from "@snapfire/fsr/host";
import { t } from "@snapfire/fsr/host";

export async function load({ params, services, session }: Ctx<"/agents/{id}">) {
  const id = BigInt(params.id);
  return { agent: await services.fleet.getAgent({ id }), jobs: await services.fleet.listJobs({ id }), visits: await services.fleet.getVisits({ id }), watched: Boolean(session.watching[params.id]) };
}

export const meta = ({ data }: { data: { agent: { name: string } } }) => ({
  title: t("meta.agent", { name: data.agent.name }),
  description: t("meta.agent_description", { name: data.agent.name }),
});

export const defaults = ({ data }: { data: { agent: { id: bigint } } }) => ({ "fleet/selected": String(data.agent.id) });
