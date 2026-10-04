import type { Ctx } from "@snapfire/fsr";

export async function load({ services }: Ctx) {
  const talks = await services.program.listTalks();
  const conference = await services.program.getConference();
  return { venue: conference.venue, talks: talks.map((t) => ({ id: t.id, title: t.title, room: t.room, starts: t.starts })) };
}
