export default function Page() {
  const n = 3;
  const badge = n > 2 ? <strong>many</strong> : <em>few</em>;
  return <p>{badge}</p>;
}
