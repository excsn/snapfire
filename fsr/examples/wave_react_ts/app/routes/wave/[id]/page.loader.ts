import type { Ctx, DataOf, MetaCtx } from "@snapfire/fsr";

export async function load({ params, services, session }: Ctx<"/wave/{id}">) {
  session.waves = { ...(session.waves ?? {}), [params.id]: true };
  const wave = await services.waves.getWave({ id: params.id });
  return { wave, me: session.name };
}

export const meta = ({ data }: MetaCtx<DataOf<typeof load>>) => ({ title: `${data.wave.title} · Waves` });
