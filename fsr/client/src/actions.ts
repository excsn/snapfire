import { decodeValue, encodeValue, SfValue } from "./values.js";

export class ActionFailure extends Error {
  readonly kind: string;

  constructor(kind: string, message: string) {
    super(message);
    this.name = "ActionFailure";
    this.kind = kind;
  }
}

/** A callable for a stable action id. The client holds references, not URLs. */
export function action(id: string): (input?: SfValue) => Promise<SfValue> {
  return async (input: SfValue = {}) => {
    const res = await fetch(`/_sf/action/${encodeURIComponent(id)}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(encodeValue(input)),
    });
    const body = await res.json();
    if (!res.ok) {
      throw new ActionFailure(body.kind ?? "internal", body.message ?? res.statusText);
    }
    return decodeValue(body);
  };
}
