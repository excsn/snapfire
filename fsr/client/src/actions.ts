import { refresh } from "./navigator.js";
import { decodeValue, encodeValue, SfValue } from "./values.js";

export class ActionFailure extends Error {
  readonly kind: string;

  constructor(kind: string, message: string) {
    super(message);
    this.name = "ActionFailure";
    this.kind = kind;
  }
}

/** The failure a response is: the `{ kind, message }` body a host answers with, or the status and the text for a body that is not one, such as a proxy's or a CSRF refusal in plain text. */
function failure(status: number, statusText: string, text: string): ActionFailure {
  try {
    const body = JSON.parse(text) as { kind?: unknown; message?: unknown };
    if (body !== null && typeof body === "object" && typeof body.kind === "string") {
      return new ActionFailure(body.kind, typeof body.message === "string" ? body.message : statusText);
    }
  } catch {}
  return new ActionFailure(kindOf(status), text.trim() || statusText || `HTTP ${status}`);
}

/** The failure kind a bare status stands for: the host's status per kind read backwards, plus 403 as unauthorized. */
function kindOf(status: number): string {
  switch (status) {
    case 400:
      return "invalid";
    case 401:
    case 403:
      return "unauthorized";
    case 404:
      return "not_found";
    case 409:
      return "conflict";
    case 503:
      return "unavailable";
    case 504:
      return "timeout";
    default:
      return "internal";
  }
}

/** A callable for a stable action id. The client holds references, not URLs. A successful call revalidates the current route by default, so mutated segments refresh in place. The document's path rides as `x-sf-from`, which is how the server gives the action the document's locale. */
export function action(id: string, opts?: { revalidate?: boolean }): (input?: SfValue) => Promise<SfValue> {
  return async (input: SfValue = {}) => {
    const headers: Record<string, string> = { "content-type": "application/json" };
    if (typeof window !== "undefined") headers["x-sf-from"] = `${window.location.pathname}${window.location.search}`;
    const res = await fetch(`/_sf/action/${encodeURIComponent(id)}`, {
      method: "POST",
      headers,
      body: JSON.stringify(encodeValue(input)),
    });
    const text = await res.text();
    if (!res.ok) {
      throw failure(res.status, res.statusText, text);
    }
    const result = decodeValue(JSON.parse(text));
    if (opts?.revalidate !== false) {
      await refresh();
    }
    return result;
  };
}
