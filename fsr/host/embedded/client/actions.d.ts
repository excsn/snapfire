import { SfValue } from "./values.js";
export declare class ActionFailure extends Error {
	readonly kind: string;
	constructor(kind: string, message: string);
}
/** A callable for a stable action id. The client holds references, not URLs. A successful call revalidates the current route by default, so mutated segments refresh in place. The document's path rides as `x-sf-from`, which is how the server gives the action the document's locale. */
export declare function action(id: string, opts?: {
	revalidate?: boolean;
}): (input?: SfValue) => Promise<SfValue>;
