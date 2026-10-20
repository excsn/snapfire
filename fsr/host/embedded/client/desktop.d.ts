/** A filter in the open dialog: its label and the extensions it shows, without the dot. */
export interface FileFilter {
	name: string;
	extensions: string[];
}
export interface OpenFileOptions {
	multiple?: boolean;
	filters?: FileFilter[];
}
/** What a desktop window offers the page beyond the web: `fsr --desktop` answers it in the process that opened the window. */
export interface Desktop {
	isDesktop(): boolean;
	openFile(options?: OpenFileOptions): Promise<string[] | null>;
	openExternal(url: string): Promise<void>;
}
export declare const desktop: Desktop;
