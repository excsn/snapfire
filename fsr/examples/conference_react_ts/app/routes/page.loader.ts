import type { Ctx } from "@snapfire/fsr";

export async function load({ query, session, services }: Ctx) {
  const talks = await services.program.listTalks();
  const conference = await services.program.getConference();
  const track = query.track ?? "all";
  const shown = track === "all" ? talks : talks.filter((t) => t.track === track);
  return { track, tracks: conference.tracks, talks: shown, saved: Object.keys(session.saved) };
}
