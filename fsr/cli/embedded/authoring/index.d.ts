export type FailureKind =
  | "invalid"
  | "unauthorized"
  | "payment_required"
  | "forbidden"
  | "not_found"
  | "method_not_allowed"
  | "not_acceptable"
  | "proxy_authentication_required"
  | "request_timeout"
  | "conflict"
  | "gone"
  | "length_required"
  | "precondition_failed"
  | "content_too_large"
  | "uri_too_long"
  | "unsupported_media_type"
  | "range_not_satisfiable"
  | "expectation_failed"
  | "misdirected_request"
  | "unprocessable_content"
  | "locked"
  | "failed_dependency"
  | "too_early"
  | "upgrade_required"
  | "precondition_required"
  | "too_many_requests"
  | "request_header_fields_too_large"
  | "unavailable_for_legal_reasons"
  | "internal"
  | "not_implemented"
  | "bad_gateway"
  | "unavailable"
  | "timeout"
  | "http_version_not_supported"
  | "variant_also_negotiates"
  | "insufficient_storage"
  | "loop_detected"
  | "not_extended"
  | "network_authentication_required";

export interface Identity {
  subject: string;
  claims: Record<string, unknown>;
}

/** What a loader or action body sees. Generated types narrow `params`, `session` and `services` per application. */
export interface Ctx<Params = Record<string, string>, Session = Record<string, any>, Services = Record<string, Record<string, (args?: Record<string, unknown>) => Promise<any>>>> {
  params: Params;
  query: Record<string, string>;
  session: Session;
  identity: Identity | null;
  /** The request's locale as the configuration spells it, `fr_FR` or `fr`; `en` without a `[locales]` section. */
  locale: string;
  services: Services;
  now: bigint;
}

export interface ActionCtx<Input, Params = Record<string, string>, Session = Record<string, any>, Services = Ctx["services"]> extends Ctx<Params, Session, Services> {
  input: Input;
}

/** Fails the body with a kind the runtime maps onto a status. A statement, never an expression. */
export function fail(kind: FailureKind, message: string): never;

/** Ends an action and sends its caller to `to`, a path on this origin: a form post is answered with a 303 there and a call from `actions` navigates there. A statement, never an expression; only an action holds one. */
export function redirect(to: string): never;

/** Declares an action. The type argument names the input type the build emits into the contract. */
export function action<Input = void, Out = unknown>(body: (ctx: ActionCtx<Input>) => Promise<Out>): (ctx: ActionCtx<Input>) => Promise<Out>;
