import { Island, Link } from "@snapfire/fsr-client/react";

import type { RecipeIdProps } from "@generated/client";
import PlanRecipe from "@src/ui/PlanRecipe.vue";
import Scaler from "@src/ui/Scaler.vue";

export default function RecipePage({ recipe, sameCourse, planned }: RecipeIdProps) {
  return (
    <article className="page recipe">
      <p className="when">
        {recipe.course} · {recipe.cook} · {recipe.minutes} minutes · serves {recipe.serves}
      </p>
      <h2>{recipe.title}</h2>
      <Island when="load">
        <PlanRecipe id={recipe.id} planned={planned} />
      </Island>
      <p className="method">{recipe.method}</p>
      <section className="same-course">
        <h3>Also under {recipe.course}</h3>
        {sameCourse.length === 0 ? (
          <p className="quiet">Nothing else yet.</p>
        ) : (
          <ul>
            {sameCourse.map((other) => (
              <li key={other.id}>
                <Link href={`/recipe/${other.id}`}>{other.title}</Link>
                <span className="by">{other.cook}</span>
              </li>
            ))}
          </ul>
        )}
      </section>
      <div className="filler" />
      <Island when="visible">
        <Scaler serves={recipe.serves} ingredients={recipe.ingredients} />
      </Island>
    </article>
  );
}
