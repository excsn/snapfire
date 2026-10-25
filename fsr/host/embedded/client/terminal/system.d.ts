import { system } from "../std.js";
export type { SpawnOptions, Spawned, SpawnedInput, SpawnedOutput } from "../std.js";
export { Scope } from "../std.js";
/** The environment variable `name`, null when it is not set. Runs in a terminal program only. */
export declare const env: typeof system.env;
/** The home directory. Runs in a terminal program only. */
export declare const home: typeof system.home;
/** The operating system: `macos`, `linux`, `windows` or another name Rust gives one. Runs in a terminal program only. */
export declare const platform: typeof system.platform;
/** The temporary directory, no trailing `/`. Runs in a terminal program only. */
export declare const tempDir: typeof system.tempDir;
/** The line ending: `"\r\n"` on Windows, `"\n"` elsewhere. Runs in a terminal program only. */
export declare const lineEnd: typeof system.lineEnd;
/** The working directory. Runs in a terminal program only. */
export declare const cwd: typeof system.cwd;
/** The resident set of the running page, in bytes. Runs in a terminal program only. */
export declare const memory: typeof system.memory;
/** Starts `command` with `args`, its output piped, as a process the page hears from: `stdout.on("data", f)` and `stderr.on("data", f)` hear its output, `on("exit", f)` and `on("close", f)` its code and signal and `on("error", f)` a failure to start. `options` are `cwd`, `env`, `shell` and `stdio: "inherit"` for a process that owns the terminal. Terminal pages only. Runs in a terminal program only. */
export declare const spawn: typeof system.spawn;
