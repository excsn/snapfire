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

function call(method: string, args: unknown): Promise<unknown> {
  const held = (globalThis as { __sfDesktop?: Bridge }).__sfDesktop;
  if (!held) return Promise.reject(new Error(`${method} runs only in an fsr --desktop window`));
  return held.call(method, args);
}

/** The system's open dialog: the chosen paths, `null` when the user cancels. */
export async function openFile(options: OpenFileOptions = {}): Promise<string[] | null> {
  return (await call("openFile", options)) as string[] | null;
}

/** Opens an `http`, `https` or `mailto` link in the system's browser or mail app. */
export async function openExternal(url: string): Promise<void> {
  await call("openExternal", { url });
}
