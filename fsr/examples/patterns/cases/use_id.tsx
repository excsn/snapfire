import { useId } from "react";
function Field({ label }: { label: string }) {
  const id = useId();
  return <p><label htmlFor={id}>{label}</label><input id={id} /></p>;
}
export default function Page() {
  const name = useId();
  const hint = useId();
  return (
    <form>
      <label htmlFor={name}>Name</label>
      <input id={name} aria-describedby={hint} />
      <span id={hint}>required</span>
      {["a", "b"].map((label) => <Field key={label} label={label} />)}
    </form>
  );
}
