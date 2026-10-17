import { useId, useState } from "react";

function Field({ label }: { label: string }) {
  const id = useId();
  return (
    <p className="field">
      <label htmlFor={id}>{label}</label>
      <input id={id} />
    </p>
  );
}

export default function Fields() {
  const first = useId();
  const hint = useId();
  const [shown, setShown] = useState(false);
  return (
    <div className="fields">
      <label className="first" htmlFor={first}>
        First
      </label>
      <input id={first} aria-describedby={hint} />
      <span className="hint" id={hint}>
        {shown ? "shown" : "hidden"}
      </span>
      {["Second", "Third"].map((label) => (
        <Field key={label} label={label} />
      ))}
      <button className="show" onClick={() => setShown(true)}>
        show
      </button>
    </div>
  );
}
