import { Link } from "@snapfire/fsr-authoring/template";

export default function Other({ by }: { by: string }) {
  return (
    <>
      <p className="page-by">{by}</p>
      <Link href="/streamed" className="to-streamed">
        streamed
      </Link>
    </>
  );
}
