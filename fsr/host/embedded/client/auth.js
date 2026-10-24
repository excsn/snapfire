export class AuthFailure extends Error {
    kind;
    constructor(kind, message){
        super(message);
        this.name = "AuthFailure";
        this.kind = kind;
    }
}
function here() {
    return typeof window === "undefined" ? "/" : `${window.location.pathname}${window.location.search}`;
}
function query(opts) {
    const params = new URLSearchParams({
        return_to: opts.returnTo ?? here()
    });
    if (opts.method !== undefined) params.set("method", opts.method);
    return params.toString();
}
async function step(res) {
    const text = await res.text();
    let body = {};
    try {
        body = JSON.parse(text);
    } catch  {
        throw new AuthFailure(res.ok ? "invalid" : res.status === 403 ? "denied" : "invalid", text.trim() || `HTTP ${res.status}`);
    }
    if (!res.ok || typeof body.error === "string") throw new AuthFailure(body.error ?? "invalid", body.message ?? `HTTP ${res.status}`);
    switch(body.next){
        case "redirect":
        case "complete":
            return {
                next: body.next,
                to: body.to ?? "/"
            };
        case "present":
            return {
                next: "present",
                kind: body.kind ?? "",
                data: body.data ?? {}
            };
        default:
            throw new AuthFailure("invalid", `the host answered a step of ${String(body.next)}`);
    }
}
export async function answer(params) {
    const res = await fetch("/auth/callback", {
        method: "POST",
        headers: {
            accept: "application/json",
            "content-type": "application/json"
        },
        body: JSON.stringify(params)
    });
    return step(res);
}
export async function signIn(opts = {}) {
    const res = await fetch(`/auth/login?${query(opts)}`, {
        headers: {
            accept: "application/json"
        }
    });
    return run(await step(res), opts);
}
export async function enrol(csrf, opts = {}) {
    const res = await fetch(`/auth/enrol?${query(opts)}`, {
        method: "POST",
        headers: {
            accept: "application/json",
            "x-sf-csrf": csrf
        }
    });
    return run(await step(res), opts);
}
async function run(first, opts) {
    let current = first;
    for(;;){
        if (current.next !== "present") {
            if (opts.navigate !== false && typeof window !== "undefined") window.location.assign(current.to);
            return current;
        }
        let reply;
        if (current.kind === "webauthn.get" || current.kind === "webauthn.create") {
            reply = {
                credential: await ceremony(current.kind, current.data)
            };
        } else if (opts.present !== undefined) {
            reply = await opts.present({
                kind: current.kind,
                data: current.data
            });
        }
        if (reply === undefined) return current;
        current = await answer(reply);
    }
}
const BINARY_OPTIONS = [
    "challenge"
];
function fromBase64url(text) {
    const padded = text.replace(/-/g, "+").replace(/_/g, "/") + "===".slice((text.length + 3) % 4);
    const bytes = Uint8Array.from(atob(padded), (c)=>c.charCodeAt(0));
    return bytes.buffer;
}
function toBase64url(buffer) {
    if (buffer === null || buffer === undefined) return null;
    let text = "";
    for (const byte of new Uint8Array(buffer))text += String.fromCharCode(byte);
    return btoa(text).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}
function decodeOptions(options) {
    const out = {
        ...options
    };
    for (const key of BINARY_OPTIONS)if (typeof out[key] === "string") out[key] = fromBase64url(out[key]);
    const user = out.user;
    if (user !== undefined && typeof user.id === "string") out.user = {
        ...user,
        id: fromBase64url(user.id)
    };
    for (const list of [
        "allowCredentials",
        "excludeCredentials"
    ]){
        const entries = out[list];
        if (Array.isArray(entries)) out[list] = entries.map((entry)=>typeof entry.id === "string" ? {
                ...entry,
                id: fromBase64url(entry.id)
            } : entry);
    }
    return out;
}
function encodeCredential(credential) {
    const fields = {
        clientDataJSON: toBase64url(credential.response.clientDataJSON)
    };
    if ("attestationObject" in credential.response) {
        const created = credential.response;
        fields.attestationObject = toBase64url(created.attestationObject);
        if (typeof created.getTransports === "function") fields.transports = created.getTransports();
    } else {
        const asserted = credential.response;
        fields.authenticatorData = toBase64url(asserted.authenticatorData);
        fields.signature = toBase64url(asserted.signature);
        fields.userHandle = toBase64url(asserted.userHandle);
    }
    return {
        id: credential.id,
        rawId: toBase64url(credential.rawId),
        type: credential.type,
        response: fields,
        extensions: credential.getClientExtensionResults()
    };
}
async function ceremony(kind, data) {
    if (typeof navigator === "undefined" || navigator.credentials === undefined || typeof PublicKeyCredential === "undefined") {
        throw new AuthFailure("unsupported", "this browser cannot run a passkey step");
    }
    const publicKey = decodeOptions(data.publicKey ?? data);
    const extra = typeof data.mediation === "string" ? {
        mediation: data.mediation
    } : {};
    const credential = kind === "webauthn.create" ? await navigator.credentials.create({
        publicKey: publicKey
    }) : await navigator.credentials.get({
        publicKey: publicKey,
        ...extra
    });
    if (credential === null) throw new AuthFailure("denied", "the passkey prompt was dismissed");
    return encodeCredential(credential);
}
//# sourceMappingURL=auth.js.map
