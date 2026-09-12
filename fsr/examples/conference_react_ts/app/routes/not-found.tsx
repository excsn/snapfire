import { Link } from "@snapfire/fsr-client/react";

export default function NotFound() {
  return (
    <section className="page">
      <h2>No such page</h2>
      <p>
        <Link href="/">Back to the day</Link>
      </p>
    </section>
  );
}
