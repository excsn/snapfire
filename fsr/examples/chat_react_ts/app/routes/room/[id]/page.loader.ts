import type { Ctx, DataOf, MetaCtx } from "@snapfire/fsr";

export async function load({ params, services, session }: Ctx<"/room/{id}">) {
  session.rooms = { ...(session.rooms ?? {}), [params.id]: true };
  const transcript = await services.rooms.getRoom({ id: params.id });
  return { room: transcript.room, messages: transcript.messages, me: session.name };
}

export const meta = ({ data }: MetaCtx<DataOf<typeof load>>) => ({ title: `${data.room.name} · Rooms` });
