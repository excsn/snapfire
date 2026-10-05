import type { Ctx } from "@snapfire/fsr";

export async function load(_: Ctx<"/streamed">) {
  return { by: "alpha" };
}

export const store = ({ data }: { data: { by: string } }) => ({ "repro/slot": data.by });
