import { useStore } from "@snapfire/fsr-client/react";

import { owner } from "@src/store";

export default function Bump() {
  const [value, setValue] = useStore(owner, "none");
  return (
    <button className="bump" onClick={() => setValue("clicked")}>
      {value}
    </button>
  );
}
