/** A step of a sign-in as the host answers it: where the browser goes, what the page acts on or where the flow landed. */
export type AuthStep = {
	next: "redirect";
	to: string;
} | {
	next: "present";
	kind: string;
	data: Record<string, unknown>;
} | {
	next: "complete";
	to: string;
};
/** A step the host refused: `denied` when the visitor is not permitted, `invalid` for a malformed answer or no flow in progress, `unsupported` for a passkey step this browser cannot run. */
export declare class AuthFailure extends Error {
	readonly kind: string;
	constructor(kind: string, message: string);
}
export interface AuthOptions {
	/** The path on this origin the flow lands on; the current page when absent. */
	returnTo?: string;
	/** The method to use, for a provider offering several. */
	method?: string;
	/** Answers a step the page shows, such as a form's fields or a code sent elsewhere; nothing stops the loop and returns the step to the caller. Passkey steps are answered without it. */
	present?: (step: {
		kind: string;
		data: Record<string, unknown>;
	}) => Promise<Record<string, unknown> | undefined> | Record<string, unknown> | undefined;
	/** Whether a redirect or a completed flow moves the browser; `true` when absent. */
	navigate?: boolean;
}
/** Posts an answer to the step in progress and returns the next step without following it. */
export declare function answer(params: Record<string, unknown>): Promise<AuthStep>;
/** Begins a sign-in and runs it: follows a redirect, runs a passkey ceremony, hands any other step to `present` and posts its answer, until the flow completes or leaves the page. Returns the last step. */
export declare function signIn(opts?: AuthOptions): Promise<AuthStep>;
/** Begins adding a method, a passkey or a linked account, to whoever is signed in and runs it as `signIn` does. `csrf` is the page's `csrf_token`. */
export declare function enrol(csrf: string, opts?: AuthOptions): Promise<AuthStep>;
