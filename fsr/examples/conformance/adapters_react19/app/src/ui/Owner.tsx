import { useStore } from "@snapfire/fsr-client/react";

import { owner } from "@src/store";

export default function Owner({ name }: { name: string }) {
  const [value] = useStore(owner, "none");
  return (
    <p className="owner" data-island={name}>
      {value}
    </p>
  );
}
