/** A filter in the open dialog: its label and the extensions it shows, without the dot. */
export interface FileFilter {
	name: string;
	extensions: string[];
}
export interface OpenFileOptions {
	multiple?: boolean;
	filters?: FileFilter[];
}
/** The system's open dialog: the chosen paths, `null` when the user cancels. */
export declare function openFile(options?: OpenFileOptions): Promise<string[] | null>;
/** Opens an `http`, `https` or `mailto` link in the system's browser or mail app. */
export declare function openExternal(url: string): Promise<void>;
