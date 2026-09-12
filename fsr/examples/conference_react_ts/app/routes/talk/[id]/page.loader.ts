import { fail } from "@snapfire/fsr";
import type { Ctx } from "@snapfire/fsr";

export async function load({ params, session, services }: Ctx<"/talk/{id}">) {
  const talks = await services.program.listTalks();
  const matches = talks.filter((t) => t.id === params.id);
  if (matches.length === 0) fail("not_found", "there is no talk with that id");
  const talk = matches[0];
  const alongside = talks.filter((t) => t.starts === talk.starts && t.id !== talk.id);
  return { talk, alongside, saved: session.saved[params.id] ?? false };
}

export const meta = ({ data }: { data: { talk: { title: string; speaker: string; room: string } } }) => ({
  title: `${data.talk.title} · Everything At Once`,
  description: `${data.talk.speaker}, in ${data.talk.room}`,
});
