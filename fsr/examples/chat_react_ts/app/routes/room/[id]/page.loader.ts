import type { Ctx, DataOf, MetaCtx } from "@snapfire/fsr";

export async function load({ params, services, session, native }: Ctx<"/room/{id}">) {
  session.rooms = { ...(session.rooms ?? {}), [params.id]: true };
  const transcript = await services.rooms.getRoom({ id: params.id });
  const bodies = transcript.messages.map((m) => m.body);
  return {
    room: transcript.room,
    messages: transcript.messages,
    me: session.name,
    words: native.digest.words({ bodies }),
    longest: native.digest.longest({ bodies }),
  };
}

export const meta = ({ data }: MetaCtx<DataOf<typeof load>>) => ({ title: `${data.room.name} · Rooms` });
