import { Link } from "@snapfire/fsr-authoring/template";

export default function RecipeError() {
  return (
    <section className="page">
      <h2>Not in the box</h2>
      <p>
        There is no recipe with that id. <Link href="/">Back to the box</Link>.
      </p>
    </section>
  );
}
