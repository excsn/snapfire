import { Link } from "@snapfire/fsr-authoring/template";

export default function ToolError() {
  return (
    <section className="page">
      <h2>Not in the shed</h2>
      <p>
        There is no tool with that id. <Link href="/">Back to the shelves</Link>.
      </p>
    </section>
  );
}
