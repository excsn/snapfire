import { useId } from "react";
import { useStore } from "@snapfire/fsr-client/react";

import { owner } from "@src/store";

export default function OwnerBrowser({ name }: { name: string }) {
  const id = useId();
  const [value] = useStore(owner, "none");
  return (
    <p className="owner-unrendered" id={id} data-island={name}>
      {value}
    </p>
  );
}
