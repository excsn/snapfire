function Star() { return <b>*</b>; }
function Heart() { return <b>&lt;3</b>; }
const icons = { star: Star, heart: Heart };
export default function Page() {
  const Icon = icons["heart" as keyof typeof icons];
  return <p><Icon /> love</p>;
}
