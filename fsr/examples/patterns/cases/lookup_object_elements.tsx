export default function Page() {
  const kind = "warn";
  const icons = { info: <i className="info">i</i>, warn: <i className="warn">!</i> };
  return <p>{icons[kind]} careful</p>;
}
