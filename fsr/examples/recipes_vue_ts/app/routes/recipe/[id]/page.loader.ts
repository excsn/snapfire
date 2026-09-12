import { fail } from "@snapfire/fsr";
import type { Ctx } from "@snapfire/fsr";

export async function load({ params, session, services }: Ctx<"/recipe/{id}">) {
  const recipes = await services.kitchen.listRecipes();
  const matches = recipes.filter((r) => r.id === params.id);
  if (matches.length === 0) fail("not_found", "there is no recipe with that id");
  const recipe = matches[0];
  const sameCourse = recipes.filter((r) => r.course === recipe.course && r.id !== recipe.id);
  return { recipe, sameCourse, planned: session.planned[params.id] ?? false };
}

export const meta = ({ data }: { data: { recipe: { title: string; cook: string; minutes: number } } }) => ({
  title: `${data.recipe.title} · The Sunday Box`,
  description: `${data.recipe.cook}'s, ${data.recipe.minutes} minutes`,
});
