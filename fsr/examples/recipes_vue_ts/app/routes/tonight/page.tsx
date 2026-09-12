import { Link } from "@snapfire/fsr-authoring/template";

import type { TonightProps } from "@generated/client";

export default function TonightPage({ tonight, minutes }: TonightProps) {
  return (
    <section className="page tonight-page">
      <h2>Tonight</h2>
      {tonight.length === 0 ? (
        <p className="quiet">
          Nothing planned yet. Open <Link href="/recipe/3">a recipe</Link> and add it.
        </p>
      ) : (
        <p className="strap">{minutes} minutes at the stove, all in.</p>
      )}
      <ol className="recipe-list">
        {tonight.map((recipe) => (
          <li key={recipe.id} className="recipe-row">
            <span className="what">
              <Link href={`/recipe/${recipe.id}`} className="recipe-title">
                {recipe.title}
              </Link>
              <span className="by">
                {recipe.cook}, {recipe.minutes} minutes
              </span>
            </span>
            <span className={`course course-${recipe.course.toLowerCase()}`}>{recipe.course}</span>
          </li>
        ))}
      </ol>
      <p className="note">This page reads the session. Nothing about it is cached.</p>
    </section>
  );
}
