import type { Ctx } from "@snapfire/fsr/host";

export async function load({ query }: Ctx<"/regions">) {
  const second = query.set === "b";
  return {
    heading: second ? "revisited" : "answered",
    answers: [
      { id: "why", q: "Why?", a: second ? "Still because" : "Because" },
      { id: "how", q: "How?", a: second ? "Another way" : "Like so" },
    ],
  };
}
