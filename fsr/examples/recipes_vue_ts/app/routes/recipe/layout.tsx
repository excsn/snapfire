import { Link, type Children } from "@snapfire/fsr-authoring/template";

export default function RecipeLayout({ children }: { children: Children }) {
  return (
    <section className="recipe-shell">
      <p className="crumbs">
        <Link href="/">The box</Link>
        <span className="sep">/</span>
        <span className="here">This recipe</span>
      </p>
      {children}
    </section>
  );
}
