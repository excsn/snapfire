import type { Ctx } from "@snapfire/fsr";

export async function load({ session, services }: Ctx) {
  const box = await services.kitchen.getBox();
  return { box, planned: Object.keys(session.planned).length };
}

export const store = ({ data }: { data: { planned: number } }) => ({ "tonight/planned": data.planned });
