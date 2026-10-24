/** A step of a sign-in as the host answers it: where the browser goes, what the page acts on or where the flow landed. */
export type AuthStep =
  | { next: "redirect"; to: string }
  | { next: "present"; kind: string; data: Record<string, unknown> }
  | { next: "complete"; to: string };

/** A step the host refused: `denied` when the visitor is not permitted, `invalid` for a malformed answer or no flow in progress, `unsupported` for a passkey step this browser cannot run. */
export class AuthFailure extends Error {
  readonly kind: string;

  constructor(kind: string, message: string) {
    super(message);
    this.name = "AuthFailure";
    this.kind = kind;
  }
}

export interface AuthOptions {
  /** The path on this origin the flow lands on; the current page when absent. */
  returnTo?: string;
  /** The method to use, for a provider offering several. */
  method?: string;
  /** Answers a step the page shows, such as a form's fields or a code sent elsewhere; nothing stops the loop and returns the step to the caller. Passkey steps are answered without it. */
  present?: (step: { kind: string; data: Record<string, unknown> }) => Promise<Record<string, unknown> | undefined> | Record<string, unknown> | undefined;
  /** Whether a redirect or a completed flow moves the browser; `true` when absent. */
  navigate?: boolean;
}

function here(): string {
  return typeof window === "undefined" ? "/" : `${window.location.pathname}${window.location.search}`;
}

function query(opts: AuthOptions): string {
  const params = new URLSearchParams({ return_to: opts.returnTo ?? here() });
  if (opts.method !== undefined) params.set("method", opts.method);
  return params.toString();
}

async function step(res: Response): Promise<AuthStep> {
  const text = await res.text();
  let body: { next?: string; to?: string; kind?: string; data?: Record<string, unknown>; error?: string; message?: string } = {};
  try {
    body = JSON.parse(text);
  } catch {
    throw new AuthFailure(res.ok ? "invalid" : res.status === 403 ? "denied" : "invalid", text.trim() || `HTTP ${res.status}`);
  }
  if (!res.ok || typeof body.error === "string") throw new AuthFailure(body.error ?? "invalid", body.message ?? `HTTP ${res.status}`);
  switch (body.next) {
    case "redirect":
    case "complete":
      return { next: body.next, to: body.to ?? "/" };
    case "present":
      return { next: "present", kind: body.kind ?? "", data: body.data ?? {} };
    default:
      throw new AuthFailure("invalid", `the host answered a step of ${String(body.next)}`);
  }
}

/** Posts an answer to the step in progress and returns the next step without following it. */
export async function answer(params: Record<string, unknown>): Promise<AuthStep> {
  const res = await fetch("/auth/callback", { method: "POST", headers: { accept: "application/json", "content-type": "application/json" }, body: JSON.stringify(params) });
  return step(res);
}

/** Begins a sign-in and runs it: follows a redirect, runs a passkey ceremony, hands any other step to `present` and posts its answer, until the flow completes or leaves the page. Returns the last step. */
export async function signIn(opts: AuthOptions = {}): Promise<AuthStep> {
  const res = await fetch(`/auth/login?${query(opts)}`, { headers: { accept: "application/json" } });
  return run(await step(res), opts);
}

/** Begins adding a method, a passkey or a linked account, to whoever is signed in and runs it as `signIn` does. `csrf` is the page's `csrf_token`. */
export async function enrol(csrf: string, opts: AuthOptions = {}): Promise<AuthStep> {
  const res = await fetch(`/auth/enrol?${query(opts)}`, { method: "POST", headers: { accept: "application/json", "x-sf-csrf": csrf } });
  return run(await step(res), opts);
}

async function run(first: AuthStep, opts: AuthOptions): Promise<AuthStep> {
  let current = first;
  for (;;) {
    if (current.next !== "present") {
      if (opts.navigate !== false && typeof window !== "undefined") window.location.assign(current.to);
      return current;
    }
    let reply: Record<string, unknown> | undefined;
    if (current.kind === "webauthn.get" || current.kind === "webauthn.create") {
      reply = { credential: await ceremony(current.kind, current.data) };
    } else if (opts.present !== undefined) {
      reply = await opts.present({ kind: current.kind, data: current.data });
    }
    if (reply === undefined) return current;
    current = await answer(reply);
  }
}

const BINARY_OPTIONS = ["challenge"];

function fromBase64url(text: string): ArrayBuffer {
  const padded = text.replace(/-/g, "+").replace(/_/g, "/") + "===".slice((text.length + 3) % 4);
  const bytes = Uint8Array.from(atob(padded), (c) => c.charCodeAt(0));
  return bytes.buffer;
}

function toBase64url(buffer: ArrayBuffer | null | undefined): string | null {
  if (buffer === null || buffer === undefined) return null;
  let text = "";
  for (const byte of new Uint8Array(buffer)) text += String.fromCharCode(byte);
  return btoa(text).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

/** WebAuthn options as JSON carries them, base64url where the browser wants bytes, made ready for `navigator.credentials`. */
function decodeOptions(options: Record<string, unknown>): Record<string, unknown> {
  const out: Record<string, unknown> = { ...options };
  for (const key of BINARY_OPTIONS) if (typeof out[key] === "string") out[key] = fromBase64url(out[key] as string);
  const user = out.user as Record<string, unknown> | undefined;
  if (user !== undefined && typeof user.id === "string") out.user = { ...user, id: fromBase64url(user.id) };
  for (const list of ["allowCredentials", "excludeCredentials"]) {
    const entries = out[list] as Array<Record<string, unknown>> | undefined;
    if (Array.isArray(entries)) out[list] = entries.map((entry) => (typeof entry.id === "string" ? { ...entry, id: fromBase64url(entry.id) } : entry));
  }
  return out;
}

/** A credential as JSON, base64url for every byte field, which is what a relying party reads. */
function encodeCredential(credential: PublicKeyCredential): Record<string, unknown> {
  const fields: Record<string, unknown> = { clientDataJSON: toBase64url(credential.response.clientDataJSON) };
  if ("attestationObject" in credential.response) {
    const created = credential.response as AuthenticatorAttestationResponse;
    fields.attestationObject = toBase64url(created.attestationObject);
    if (typeof created.getTransports === "function") fields.transports = created.getTransports();
  } else {
    const asserted = credential.response as AuthenticatorAssertionResponse;
    fields.authenticatorData = toBase64url(asserted.authenticatorData);
    fields.signature = toBase64url(asserted.signature);
    fields.userHandle = toBase64url(asserted.userHandle);
  }
  return { id: credential.id, rawId: toBase64url(credential.rawId), type: credential.type, response: fields, extensions: credential.getClientExtensionResults() };
}

/** Runs the browser's passkey prompt for a step's options, `{ publicKey }` or the public key options themselves. */
async function ceremony(kind: string, data: Record<string, unknown>): Promise<Record<string, unknown>> {
  if (typeof navigator === "undefined" || navigator.credentials === undefined || typeof PublicKeyCredential === "undefined") {
    throw new AuthFailure("unsupported", "this browser cannot run a passkey step");
  }
  const publicKey = decodeOptions((data.publicKey as Record<string, unknown> | undefined) ?? data);
  const extra: Record<string, unknown> = typeof data.mediation === "string" ? { mediation: data.mediation } : {};
  const credential = kind === "webauthn.create"
    ? await navigator.credentials.create({ publicKey: publicKey as unknown as PublicKeyCredentialCreationOptions })
    : await navigator.credentials.get({ publicKey: publicKey as unknown as PublicKeyCredentialRequestOptions, ...extra });
  if (credential === null) throw new AuthFailure("denied", "the passkey prompt was dismissed");
  return encodeCredential(credential as PublicKeyCredential);
}
