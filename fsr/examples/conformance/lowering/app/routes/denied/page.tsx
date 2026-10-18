import { Link } from "@snapfire/fsr-authoring/template";

export default function Denied({ shelf }: { shelf: string }) {
  return (
    <section className="denied">
      <p className="shelf">{shelf}</p>
      <Link href="/denied?as=forbidden" className="staff">
        staff shelf
      </Link>
    </section>
  );
}
