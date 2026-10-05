import { Link } from "@snapfire/fsr-authoring/template";

export default function OtherPeek({ by }: { by: string }) {
  return (
    <aside className="peek">
      <p className="peek-by">{by}</p>
      <Link href="/streamed?page=layout" className="peek-close">
        close
      </Link>
    </aside>
  );
}
