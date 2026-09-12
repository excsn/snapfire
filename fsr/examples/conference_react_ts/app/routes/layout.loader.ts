import type { Ctx } from "@snapfire/fsr";
import { linkTag } from "@snapfire/fsr/head";

export async function load({ session, services }: Ctx) {
  const conference = await services.program.getConference();
  return { conference, saved: Object.keys(session.saved).length };
}

export const store = ({ data }: { data: { saved: number } }) => ({ "schedule/saved": data.saved });

export const meta = () => ({ head: [linkTag("icon", "data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 16 16'><text y='13' font-size='13'>🎤</text></svg>")] });
