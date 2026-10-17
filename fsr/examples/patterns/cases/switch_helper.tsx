function icon(kind: string) {
  switch (kind) {
    case "ok":
      return <b className="ok">ok</b>;
    case "bad":
      return <b className="bad">bad</b>;
    default:
      return null;
  }
}
export default function Page() {
  return <p>{icon("bad")}{icon("none")}</p>;
}
