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

/** The failure a response is: the `{ kind, message }` body a host answers with or the status and the text for a body that is not one, such as a proxy's or a CSRF refusal in plain text. */
function failure(status: number, statusText: string, text: string): ActionFailure {
  try {
    const body = JSON.parse(text) as { kind?: unknown; message?: unknown };
    if (body !== null && typeof body === "object" && typeof body.kind === "string") {
      return new ActionFailure(body.kind, typeof body.message === "string" ? body.message : statusText);
    }
  } catch {}
  return new ActionFailure(kindOf(status), text.trim() || statusText || `HTTP ${status}`);
}

/** The failure kind for each HTTP error status, the host's own list. */
const KINDS = new Map<number, string>([
  [400, "invalid"],
  [401, "unauthorized"],
  [402, "payment_required"],
  [403, "forbidden"],
  [404, "not_found"],
  [405, "method_not_allowed"],
  [406, "not_acceptable"],
  [407, "proxy_authentication_required"],
  [408, "request_timeout"],
  [409, "conflict"],
  [410, "gone"],
  [411, "length_required"],
  [412, "precondition_failed"],
  [413, "content_too_large"],
  [414, "uri_too_long"],
  [415, "unsupported_media_type"],
  [416, "range_not_satisfiable"],
  [417, "expectation_failed"],
  [421, "misdirected_request"],
  [422, "unprocessable_content"],
  [423, "locked"],
  [424, "failed_dependency"],
  [425, "too_early"],
  [426, "upgrade_required"],
  [428, "precondition_required"],
  [429, "too_many_requests"],
  [431, "request_header_fields_too_large"],
  [451, "unavailable_for_legal_reasons"],
  [500, "internal"],
  [501, "not_implemented"],
  [502, "bad_gateway"],
  [503, "unavailable"],
  [504, "timeout"],
  [505, "http_version_not_supported"],
  [506, "variant_also_negotiates"],
  [507, "insufficient_storage"],
  [508, "loop_detected"],
  [510, "not_extended"],
  [511, "network_authentication_required"],
]);

/** The failure kind a bare status stands for: its own, else `invalid` for a 4xx no kind names and `internal` for anything else. */
function kindOf(status: number): string {
  return KINDS.get(status) ?? (status >= 400 && status < 500 ? "invalid" : "internal");
}

/** Posts a `FormData` to an action, which is how a file reaches one: the host reads `multipart/form-data` into the same input an ordinary call carries, with each file part arriving as an `Upload`. The form must carry `_csrf`, since a form post is verified where a JSON call is not; a page reads the token from its `csrf_token` prop. Revalidates on success like `action`. */
export async function upload(id: string, form: FormData, opts?: { revalidate?: boolean }): Promise<SfValue> {
  const headers: Record<string, string> = { accept: "application/json" };
  if (typeof window !== "undefined") headers["x-sf-from"] = `${window.location.pathname}${window.location.search}`;
  const res = await fetch(`/_sf/action/${encodeURIComponent(id)}`, { method: "POST", headers, body: form });
  const text = await res.text();
  if (!res.ok) {
    throw failure(res.status, res.statusText, text);
  }
  const result = decodeValue(JSON.parse(text));
  if (opts?.revalidate !== false) {
    await refresh();
  }
  return result;
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
