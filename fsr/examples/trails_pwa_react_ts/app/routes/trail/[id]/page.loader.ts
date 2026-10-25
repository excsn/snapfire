import type { Ctx } from "@snapfire/fsr/host";
import { fail } from "@snapfire/fsr/host";
import { trails } from "@src/trails";

export async function load({ params }: Ctx<"/trail/{id}">) {
  const trail = trails.find((t) => t.id === params.id);
  if (trail === undefined) fail("not_found", "there is no trail with that id");
  return { trail };
}

export const meta = ({ data }: { data: { trail: { name: string } } }) => ({ title: data.trail.name });
