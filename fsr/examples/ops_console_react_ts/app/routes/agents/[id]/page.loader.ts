import type { Ctx } from "@snapfire/fsr";
import { t } from "@snapfire/fsr-client/std";

export async function load({ params, services, session }: Ctx<"/agents/{id}">) {
  const id = BigInt(params.id);
  return { agent: await services.fleet.getAgent({ id }), jobs: await services.fleet.listJobs({ id }), watched: session.watching[params.id] === true };
}

export const meta = ({ data }: { data: { agent: { name: string } } }) => ({
  title: t("meta.agent", { name: data.agent.name }),
  description: t("meta.agent_description", { name: data.agent.name }),
});
