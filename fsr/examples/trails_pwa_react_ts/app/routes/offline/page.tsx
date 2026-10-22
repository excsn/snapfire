import { Link } from "@snapfire/fsr/components";

export default function Offline() {
  return (
    <article>
      <h1>No signal</h1>
      <p>This page was never opened with a connection, so there is no copy of it here.</p>
      <p>
        <Link href="/">The trails you have opened</Link> are still readable.
      </p>
    </article>
  );
}
