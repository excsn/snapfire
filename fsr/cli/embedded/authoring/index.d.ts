export type FailureKind = "unauthorized" | "not_found" | "invalid" | "conflict" | "timeout" | "unavailable" | "internal";

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

/** Declares an action. The type argument names the input type the build emits into the contract. */
export function action<Input = void, Out = unknown>(body: (ctx: ActionCtx<Input>) => Promise<Out>): (ctx: ActionCtx<Input>) => Promise<Out>;
