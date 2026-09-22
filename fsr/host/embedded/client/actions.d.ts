import { SfValue } from "./values.js";
export declare class ActionFailure extends Error {
	readonly kind: string;
	constructor(kind: string, message: string);
}
/** Posts a `FormData` to an action, which is how a file reaches one: the host reads `multipart/form-data` into the same input an ordinary call carries, with each file part arriving as an `Upload`. The form must carry `_csrf`, since a form post is verified where a JSON call is not; a page reads the token from its `csrf_token` prop. Revalidates on success like `action`. */
export declare function upload(id: string, form: FormData, opts?: {
	revalidate?: boolean;
}): Promise<SfValue>;
/** A callable for a stable action id. The client holds references, not URLs. A successful call revalidates the current route by default, so mutated segments refresh in place. The document's path rides as `x-sf-from`, which is how the server gives the action the document's locale. */
export declare function action(id: string, opts?: {
	revalidate?: boolean;
}): (input?: SfValue) => Promise<SfValue>;
