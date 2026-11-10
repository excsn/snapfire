import { fail } from "@snapfire/fsr/host";
import type { Ctx } from "@snapfire/fsr/host";

/** The day of the programme and the venue's zone, which every talk's clock times are read in. */
const DAY = Temporal.PlainDate.from("2026-09-18");
const VENUE_ZONE = "Europe/London";
const ELSEWHERE = ["America/New_York", "Asia/Tokyo"];

export async function load({ params, session, services }: Ctx<"/talk/{id}">) {
  const talks = await services.program.listTalks();
  const matches = talks.filter((t) => t.id === params.id);
  if (matches.length === 0) fail("not_found", "there is no talk with that id");
  const talk = matches[0];
  const alongside = talks.filter((t) => t.starts === talk.starts && t.id !== talk.id);
  const starts = DAY.toZonedDateTime({ timeZone: VENUE_ZONE, plainTime: talk.starts });
  const minutes = Temporal.PlainTime.from(talk.starts).until(talk.ends).total("minutes");
  const elsewhere = ELSEWHERE.map((zone) => ({
    city: zone.split("/")[1].replace("_", " "),
    at: starts.withTimeZone(zone).toPlainTime().toString({ smallestUnit: "minute" }),
  }));
  return { talk, alongside, saved: session.saved[params.id] ?? false, minutes, startsAt: starts.toString(), elsewhere };
}

export const meta = ({ data }: { data: { talk: { title: string; speaker: string; room: string } } }) => ({
  title: `${data.talk.title} · Everything At Once`,
  description: `${data.talk.speaker}, in ${data.talk.room}`,
});
