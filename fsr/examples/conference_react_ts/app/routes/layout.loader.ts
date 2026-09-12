import type { Ctx } from "@snapfire/fsr";

export async function load({ session, services }: Ctx) {
  const conference = await services.program.getConference();
  return { conference, saved: Object.keys(session.saved).length };
}

export const store = ({ data }: { data: { saved: number } }) => ({ "schedule/saved": data.saved });
