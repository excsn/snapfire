import type { Ctx } from "@snapfire/fsr";

export async function load(_ctx: Ctx<"/tour">) {
  return { video: { id: "aqz-KE-bpKQ", title: "Big Buck Bunny" } };
}

export const meta = () => ({
  title: "Tour",
  description: "A page with no form, so it starts no session.",
});
