import type { Ctx } from "@snapfire/fsr";

export async function load({ session, services }: Ctx) {
  const recipes = await services.kitchen.listRecipes();
  const held = session.planned;
  const tonight = recipes.filter((r) => held[r.id] ?? false);
  const minutes = tonight.reduce((sum: number, r) => sum + r.minutes, 0);
  return { tonight, minutes };
}

export const meta = ({ data }: { data: { tonight: unknown[]; minutes: number } }) => ({
  title: `Tonight · ${data.tonight.length} ${data.tonight.length === 1 ? "recipe" : "recipes"}, ${data.minutes} minutes`,
});
