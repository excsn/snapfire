import { useActionState } from "react";

async function add(count: number) {
  return count + 1;
}

export default function Adder() {
  const [count, formAction, isPending] = useActionState(add, 0);
  return (
    <form className="adder" action={formAction}>
      <output>{count}</output>
      <button>{isPending ? "Adding" : "Add"}</button>
    </form>
  );
}
