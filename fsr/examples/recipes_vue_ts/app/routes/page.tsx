import { Link } from "@snapfire/fsr-client/react";

import type { RootProps } from "@generated/client";

export default function RecipesPage({ course, courses, recipes, planned }: RootProps) {
  return (
    <section className="page recipes">
      <h2>In the box</h2>
      <nav className="courses">
        <Link href="/" className={course === "all" ? "chip chip-on" : "chip"}>
          Everything
        </Link>
        {courses.map((name) => (
          <Link key={name} href={`/?course=${encodeURIComponent(name)}`} className={course === name ? "chip chip-on" : "chip"}>
            {name}
          </Link>
        ))}
      </nav>
      <ol className="recipe-list">
        {recipes.map((recipe) => (
          <li key={recipe.id} className="recipe-row">
            <span className="what">
              <Link href={`/recipe/${recipe.id}`} className="recipe-title">
                {recipe.title}
              </Link>
              <span className="by">
                {recipe.cook}, {recipe.minutes} minutes, serves {recipe.serves}
              </span>
            </span>
            <span className={`course course-${recipe.course.toLowerCase()}`}>{recipe.course}</span>
            {planned.includes(recipe.id) ? <span className="kept">tonight</span> : <span className="kept kept-off" />}
          </li>
        ))}
      </ol>
      {recipes.length === 0 ? <p className="quiet">Nothing under that course.</p> : null}
    </section>
  );
}
