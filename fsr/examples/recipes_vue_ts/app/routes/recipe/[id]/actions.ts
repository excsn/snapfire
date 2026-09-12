import { action, fail } from "@snapfire/fsr";
import type { ActionCtx } from "@snapfire/fsr";
import type { PlanRecipe } from "@schemas/kitchen";

export const plan = action(async ({ input, session, services }: ActionCtx<PlanRecipe>) => {
  const recipes = await services.kitchen.listRecipes();
  const known = recipes.some((r) => r.id === input.recipe_id);
  if (!known) fail("not_found", "there is no recipe with that id");
  session.planned = { ...session.planned, [input.recipe_id]: true };
  return { planned: Object.keys(session.planned).length };
});

export const unplan = action(async ({ input, session }: ActionCtx<PlanRecipe>) => {
  if (!session.planned[input.recipe_id]) fail("not_found", "that recipe is not planned for tonight");
  const kept = Object.keys(session.planned)
    .filter((id) => id !== input.recipe_id)
    .reduce((held: Record<string, boolean>, id) => ({ ...held, [id]: true }), {});
  session.planned = kept;
  return { planned: Object.keys(kept).length };
});
