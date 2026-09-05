import type { Ctx } from "@snapfire/fsr";

export async function load({ params, services }: Ctx<"/agents/{id}">) {
  const id = BigInt(params.id);
  return { agent: await services.fleet.getAgent({ id }), jobs: await services.fleet.listJobs({ id }) };
}

export const meta = ({ data }: { data: { agent: { name: string } } }) => ({
  title: `${data.agent.name} · Ops console`,
  description: `What ${data.agent.name} is running right now`,
});
