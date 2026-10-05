import type { Ctx } from "@snapfire/fsr";

export async function load(_: Ctx<"/styles">) {
  return {
    rows: [
      { name: "fsr", ms: 2 },
      { name: "react", ms: 5 },
      { name: "vue", ms: 4 },
    ],
  };
}
