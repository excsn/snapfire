import type { Ctx } from "@snapfire/fsr";

export async function load({ query, session, services }: Ctx) {
  const recipes = await services.kitchen.listRecipes();
  const box = await services.kitchen.getBox();
  const course = query.course ?? "all";
  const shown = course === "all" ? recipes : recipes.filter((r) => r.course === course);
  return { course, courses: box.courses, recipes: shown, planned: Object.keys(session.planned) };
}
