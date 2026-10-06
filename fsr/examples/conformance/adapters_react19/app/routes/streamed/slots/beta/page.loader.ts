import type { Ctx } from "@snapfire/fsr";

export async function load(_: Ctx<"/streamed">) {
  return { by: "beta" };
}

const seed = (by: string) => ({ "repro/slot": by });

export const store = ({ data }: { data: { by: string } }) => ({ ...seed(data.by) });
