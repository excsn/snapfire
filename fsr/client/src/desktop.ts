interface Bridge {
  call(method: string, args: unknown): Promise<unknown>;
}

/** A filter in the open dialog: its label and the extensions it shows, without the dot. */
export interface FileFilter {
  name: string;
  extensions: string[];
}

export interface OpenFileOptions {
  multiple?: boolean;
  filters?: FileFilter[];
}

function bridge(): Bridge | undefined {
  return (globalThis as { __sfDesktop?: Bridge }).__sfDesktop;
}

function call(method: string, args: unknown): Promise<unknown> {
  const held = bridge();
  if (!held) return Promise.reject(new Error(`desktop.${method} runs only in an fsr --desktop window`));
  return held.call(method, args);
}

/** Whether the page is in an `fsr --desktop` window, where the other calls answer. */
function isDesktop(): boolean {
  return bridge() !== undefined;
}

/** The system's open dialog. The chosen paths, or `null` when the user cancels. */
async function openFile(options: OpenFileOptions = {}): Promise<string[] | null> {
  return (await call("openFile", options)) as string[] | null;
}

/** Opens an `http`, `https` or `mailto` link in the system's browser or mail app. */
async function openExternal(url: string): Promise<void> {
  await call("openExternal", { url });
}

/** What a desktop window offers the page beyond the web: `fsr --desktop` answers it in the process that opened the window. */
export interface Desktop {
  isDesktop(): boolean;
  openFile(options?: OpenFileOptions): Promise<string[] | null>;
  openExternal(url: string): Promise<void>;
}

export const desktop: Desktop = { isDesktop, openFile, openExternal };
