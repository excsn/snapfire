# 400. Terminal apps with Ink

**For app developers.** How an application written against Ink runs in the terminal with no Node, what the build does with each part of it, the hooks Ink's API is written in, effects and timers, processes it starts, promises with the terminal it hands to another program, then how much of Ink runs today. Terminal mode is experimental.

It builds on three earlier chapters. [002](002-a-body-is-data.md) says what lowering is and why a residue names where a body cannot run; [003](003-rendered-where-it-is-cheapest.md) says how a page renders with no JavaScript engine; [102](102-components-the-server-renders.md) says what a component may say. A terminal page is one of those components drawn by a terminal instead of a browser. [Running a Terminal App](../../cli/README.USAGE.md#running-a-terminal-app) in the CLI's usage guide lists every hook and option this chapter mentions.

## Terminal mode is a feature of the CLI

The `terminal` Cargo feature of the CLI adds `fsr dev --terminal`. It is off by default:

```sh
cargo install --path fsr/cli --features terminal
cd fsr/examples/counter_terminal_ts
fsr dev app --terminal
```

The command lowers the app's root page `routes/page.tsx` and draws it in the terminal it was started from. Nothing is served and no JavaScript engine starts. Ctrl-C quits, as does a page that calls `exit()`.

## A page is an Ink component

The page imports Ink's names from `ink`, so the same file runs under Ink and Node too. `@snapfire/fsr/terminal` exports the same names for an application that does not want to name Ink:

```tsx
// app/routes/page.tsx
import { useState } from "react";
import { Box, Text, useInput } from "ink";

export default function Count() {
  const [n, setN] = useState(0);
  useInput((input) => {
    if (input === "+") setN(n + 1);
  });
  return (
    <Box borderStyle="round" paddingX={1}>
      <Text>{n}</Text>
    </Box>
  );
}
```

The build lowers this component as [chapter 102](102-components-the-server-renders.md) describes for a page the server renders: the body becomes IR and each `useInput` callback becomes a handler. The terminal holds the state. A key runs every `useInput` handler over that state, the page renders again and the terminal redraws only the cells that changed. Every component the page places runs in place with its own state, keys and timers.

A component the build cannot lower refuses the page. On the web such a component would become an island the browser renders; a terminal has no browser, so it is an error that names the component and what stopped it. `fsr dev --terminal` shows that reason in red in place of the page. Editing the file while it runs lowers it again and redraws with the state kept, the terminal's side of the dev loop in [chapter 300](300-the-build-and-the-dev-loop.md).

## Ink's hooks are TypeScript

Ink's hooks are not built into FSR. `useFocus`, `useFocusManager`, `useCursor`, `useBoxMetrics`, `useWindowSize`, `useStdout`, `useStderr`, `useApp` and `Static` are written in TypeScript over a few primitives from `@snapfire/fsr/terminal/core` and lowered like an application's own hooks. `useApp` is the whole pattern:

```tsx
import { action } from "@snapfire/fsr-client";
import { suspendTerminal } from "@snapfire/fsr/terminal/core";

export function useApp() {
  return { exit: action("$exit"), suspendTerminal };
}
```

`action("$exit")` is an act: something a handler asks the terminal to do. Writing a line above the page, starting a timer, spawning a process and moving the focus are acts too. A hook the module does not cover yet is a hook an application can write the same way.

## Effects and timers

An effect runs after the render whose dependencies it names changed, the first render included. Its body is lowered like a handler, so it may set state, dispatch, write to the terminal and start timers. A function the effect returns is its cleanup and runs before the effect runs again. [`examples/pomodoro_terminal_ts`](../../examples/pomodoro_terminal_ts/README.md) ticks only while it is running:

```tsx
useEffect(() => {
  if (!running) return;
  const tick = setInterval(() => {
    setLeft((left) => left - 1);
  }, 1000);
  return () => clearInterval(tick);
}, [running]);
```

The callback is a continuation: a handler of the component named for where it was written, carrying the names it reads. Pausing changes `running`, so the cleanup clears the interval before the effect runs again and starts nothing.

An effect only a browser could run, writing `document.title` for one, is left out.

## Files, paths and the machine

A terminal tool reads its own configuration before it draws, so a page may read the machine it runs on through three members of `@snapfire/fsr-client/std`. `file` reads and writes files: `read`, `write`, `exists`, `list`, `makeDir` and `stat`, which answers `{ isFile, isDirectory, size, mtimeMs }`. `path` joins and splits POSIX paths: `join`, `resolve`, `normalize`, `dirname`, `basename`, `extname`, `relative` and `isAbsolute`. `system` reads the machine: `env(name)`, `home()`, `platform()`, `tempDir()`, `lineEnd()`, `cwd()` and `memory()`, the resident set in bytes. Each answers at once, so an `await` of one continues with its value. Ink's `useStdout().stdout` is the terminal's size, its `on("resize", f)` running `f` when the size changes, with `off` in the effect's cleanup taking it off; `useApp().exit(code)` ends the page with that exit code.

```tsx
import { file, path, system } from "@snapfire/fsr-client/std";

const configPath = path.join(system.home(), ".config", "cards", "config.json");
const config = file.exists(configPath) ? JSON.parse(file.read(configPath)) : { deck: "default" };
```

`config` is evaluated once, when the session opens; it is not read again on each render. A missing file throws an error whose `code` is `ENOENT`, so a `try` around the read with a default in its `catch` works. A member that fails as it is awaited inside a `try` runs the `catch`, as a rejected promise does. The same members on a web page are refused, since a browser has no files: the build says which reach the page lacks. `fsr migrate` rewrites an Ink application's imports of Node's `fs`, `path` and `os` and its reads of `process` onto these members; a page that still imports one of Node's modules or reads `process` is refused.

`node:crypto` gives `randomUUID()`, `randomBytes(n)` with `toString("hex")` or `toString("base64")` reading the bytes and `createHash("sha256").update(part).digest("hex")`, read as one call. `performance.now()` counts milliseconds from the process start, so two reads give a duration. An `AsyncLocalStorage` from `node:async_hooks` held in a module `const` scopes a value to a call: `storage.run(value, fn)` binds it while `fn` runs and `storage.getStore()` reads it, `undefined` outside any `run`. Each crypto member is a member of the `crypto` extension, so an application's own Rust under `ext/` may register another implementation under the same name. `log.info(message, fields)` from `@snapfire/fsr-client/std`, with `trace`, `debug`, `warn`, `error` and `fatal` beside it, writes a structured line through the host's `tracing` output under the target `fsr::app`. `text.frontMatter(source, { delimiters })` from the same module reads a YAML block at the top of a text into `{ data, content }` and `text.withFrontMatter(content, data, { delimiters })` writes one.

A function that writes fields of an object it is passed changes the caller's object; a callback of an array method that changes its item is refused, as is an async function that writes a field of its parameter. `fsr migrate` rewrites a callback that only writes fields of its parameter to return the changed object, `c => ({ ...c, n: 1 })`; the function that calls it is rewritten to take what it returns.

## Processes

An effect or a handler may start a process with `spawn` from `node:child_process`. Its output is piped to the terminal. Each listener is a continuation like a timer's callback, so it may call the component's setters. A local the listeners write keeps its value from one report to the next. Each run of the effect gets its own. [`examples/monitor_terminal_ts`](../../examples/monitor_terminal_ts/README.md) reads a command for whichever tab is chosen:

```tsx
useEffect(() => {
  let read = "";
  const child = spawn("sh", ["-c", TABS[chosen].command]);
  child.stdout.on("data", (chunk) => {
    read = read + chunk;
  });
  child.on("exit", (code) => {
    setRunning(false);
    if (code === 0) setOutput(stripAnsi(read));
    else setFailure(`exited with ${code}`);
  });
}, [chosen]);
```

`strip-ansi`, `string-width`, `wrap-ansi` and `ms` run natively, since a terminal tool reaches for them constantly. `ink-spinner` with the `cli-spinners` it reads and `ink-text-input` are provided as TypeScript modules written as the packages are, so `<Spinner />` runs on the same timers as any other component and `<TextInput />` reads keys with `useInput` as any other component does. Other npm packages are not followed; [chapter 301](301-dependencies-without-npm.md) covers how an application gets its dependencies without npm.

## Promises and handing over the terminal

`new Promise`, `await`, `try`, `catch` and `finally` run as they do in Node. An async function runs to its first `await`; the rest of it is a continuation the terminal runs when the promise settles. The `await` is a statement of its own, a `const`'s value or an assignment, `cached = await readConfig()`. A `for...of` over an array, a counted `for` or a `while` whose body awaits runs each pass in turn, `for (const file of files) { const text = await readFile(file, "utf8"); … }`; `continue` and `return` work at the top of the body, `if (skip) continue;` and `if (found) return name;` among them; `break` inside such a loop is refused. That holds for one an effect or a handler declares as a `const` and calls by name, `const run = async () => { … }; run();`, as for one called where it is written. A rejected promise inside a `try` runs the `catch`, then the `finally` when there is one, then what follows the `try`. One that resolves runs the rest of the `try`, then the `finally`. A throw at the end of an async function rejects its promise. In a `catch`, `error instanceof Error` is true for a thrown `Error` and for the error a native member throws, so `if (error instanceof Error && error.code === "ENOENT")` reads as in Node. Three shapes are still refused in async code: a `try` that awaits inside another `try`, a destructuring pattern as the `catch` parameter and a `return` inside a `try` whose `finally` comes after an `await`. A component that throws a pending promise under `Suspense` shows the fallback until the promise settles.

`useApp().suspendTerminal(fn)` gives up the screen while the promise `fn` returns is pending. A process spawned inside it with `stdio: "inherit"` owns the terminal, which is how an Ink tool opens an editor. [`examples/launcher_terminal_ts`](../../examples/launcher_terminal_ts/README.md) runs `$EDITOR`, `less` and a shell this way. It pipes the commands that do not need the terminal into a panel.

## Bringing an Ink application in

`fsr migrate` reads an application whose `package.json` depends on `ink` and writes it as an FSR project:

```sh
fsr migrate ./my-ink-tool ./my-fsr-tool
```

The sources under `source/` or `src/` go to `app/src/` and a top-level `test.ts` or a `*.test.*` file to `app/tests/`. The entry that calls Ink's `render` is left behind, since `app/routes/page.tsx` renders the same component instead.

When `render` passed the component props, `app/routes/page.loader.ts` is written as well: the statements before `render` in the function that rendered become the loader's body and each prop becomes a field of its return, which the page gives to the component from `RootProps`. The loader runs once when the terminal opens, with the command line after `--` as `params`, so `fsr dev app --terminal -- --limit 3` reaches it as `params.limit`.

```tsx
import Ui from "@src/ui";

export default function Page() {
  return <Ui />;
}
```

An import written through the application's own `tsconfig.json` `paths`, `~/core/init` or `@/ui/Board`, is rewritten to `@src/core/init`, FSR's alias for that directory. A file next door is reached by a relative path instead. The alias map itself is not carried over: the project's tsconfig is FSR's, which chapter [302](302-imports-and-aliases.md) describes. What the entry read from the command line and passed as props, `render(<Ui limit={cli.flags.limit} />)`, is a note naming the props; `page.tsx` renders the component without them. `ink`, `react` and the packages FSR provides are not listed in the report; the rest of `dependencies` that a kept file imports are, for `fsr add`. A package FSR has a standard member for is rewritten to the member: `matter(source, options)` from `gray-matter` becomes `text.frontMatter(source, options)` and `matter.stringify({ content }, data, options)` becomes `text.withFrontMatter(content, data, options)`, through `await import("gray-matter")` too. A file that uses `gray-matter` any other way, `matter.read(path)` for one, keeps its import and gets a note. A logger made with `pino(options)` or winston's `createLogger(options)` becomes `log`: `logger.info({ id }, "running")` is `log.info("running", { id })`, a `logger.child({ module })` merges its fields into each call, winston's `http` and `verbose` write at `debug` and `silly` at `trace`. The options are dropped with a note, since the host's `tracing` filter decides which lines are written. A logger that is exported, passed around or used for anything but a level call, `logger.flush()` for one, leaves its file as written with a note.

## How much of Ink runs

Ink's own 29 examples are the measure. 26 run in the terminal with no change: they lower, draw their first frame and step on their timers and keys as they do under Ink. None is refused. Three are not run by design. `jest` is built on `p-queue` with `delay` and `table` on `@faker-js/faker`, npm packages the build does not follow. `router` is built on `react-router`, whose routes FSR's own routes stand in for ([chapter 100](100-routes-loaders-and-pages.md)). [The Ink corpus](../corpus/ink.md) lists every example and how it is run and says how to [run an Ink application of your own](../corpus/ink.md#running-an-ink-application-outside-the-corpus) to see its first frame or the first thing it needs. The [terminal examples](../../examples/README.md) are six applications written to exercise what the corpus does not: [counter](../../examples/counter_terminal_ts/README.md), [tasks](../../examples/tasks_terminal_ts/README.md), [log](../../examples/log_terminal_ts/README.md), [launcher](../../examples/launcher_terminal_ts/README.md), [monitor](../../examples/monitor_terminal_ts/README.md) and [pomodoro](../../examples/pomodoro_terminal_ts/README.md).

`Transform`, `measureElement` and pages other than `routes/page.tsx` are not supported yet.

## Lab

Run the pomodoro example:

```sh
cd fsr/examples/pomodoro_terminal_ts
fsr dev app --terminal
```

Press space to start it, then again to pause it. The clock stops. Now delete the `return () => clearInterval(tick);` line from `app/routes/page.tsx` and save. The page redraws with its state kept. Start and pause it again: the clock keeps counting while it says paused, because nothing stopped the interval the first run started. Start it once more and it counts two seconds for every one, since a second interval now runs beside the first. That is what Ink does with the same code; the cleanup is the only thing that stops a timer.

Put the line back, then add `import { measureElement } from "ink";` and call `measureElement(null)` inside the `Bar` component. The page is replaced by the reason it cannot run, naming `Bar` and `measureElement`.
