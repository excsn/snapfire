import type { Ctx } from "@snapfire/fsr/host";
import { jsonLd, og, scriptTag } from "@snapfire/fsr/head";

export async function load({ query, services }: Ctx) {
  const talks = await services.program.listTalks();
  const conference = await services.program.getConference();
  const track = query.track ?? "all";
  const shown = track === "all" ? talks : talks.filter((t) => t.track === track);
  return { venue: conference.venue, track, tracks: conference.tracks, talks: shown.map((t) => ({ id: t.id, title: t.title, room: t.room, starts: t.starts })) };
}

export const meta = () => ({
  title: "The venue",
  head: [scriptTag("/static/demo/venue-map.js", "interaction"), og("type", "place"), jsonLd('{"@type":"Place"}')],
});
