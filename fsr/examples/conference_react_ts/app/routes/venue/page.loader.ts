import type { Ctx } from "@snapfire/fsr/host";
import { scriptTag } from "@snapfire/fsr/head";

export async function load({ services }: Ctx) {
  const talks = await services.program.listTalks();
  const conference = await services.program.getConference();
  return { venue: conference.venue, talks: talks.map((t) => ({ id: t.id, title: t.title, room: t.room, starts: t.starts })) };
}

export const meta = () => ({
  head: [scriptTag("/static/demo/venue-map.js", "interaction")],
});
