import { Link } from "@snapfire/fsr-authoring/template";

export default function NotFound() {
  return (
    <section className="page">
      <h2>No such page</h2>
      <p>
        <Link href="/">Back to the box</Link>
      </p>
    </section>
  );
}
