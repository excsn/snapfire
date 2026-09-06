import type { Ctx } from "@snapfire/fsr";
import { CHAPTERS } from "@src/docs/guide";

export async function load(_ctx: Ctx) {
  return {
    chapters: CHAPTERS.map((c) => ({
      slug: c.slug,
      number: c.number,
      title: c.title,
      section: c.section,
      audience: c.audience,
    })),
  };
}

export const meta = () => ({
  title: "The FSR guide · SnapFire",
  description: "Every chapter of the SnapFire FSR guide: foundations, the application, the host, tooling and reference.",
});
