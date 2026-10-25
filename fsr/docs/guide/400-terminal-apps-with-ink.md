# 400. Terminal apps with Ink

**For app developers.** How an application written against Ink runs in the terminal with no Node, what the build does with each part of it, the hooks Ink's API is written in, effects and timers, processes it starts, promises with the terminal it hands to another program, then how much of Ink runs today. Terminal mode is experimental.

It builds on three earlier chapters. [002](002-a-body-is-data.md) says what lowering is and why a residue names where a body cannot run; [003](003-rendered-where-it-is-cheapest.md) says how a page renders with no JavaScript engine; [102](102-components-the-server-renders.md) says what a component may say. A terminal page is one of those components drawn by a terminal instead of a browser. [Running a Terminal App](../../cli/README.USAGE.md#running-a-terminal-app) in the CLI's usage guide lists every hook and option this chapter mentions.

## Terminal mode is a feature of the CLI

The `terminal` Cargo feature of the CLI lets `fsr dev` run a terminal app. It is off by default. An app is a terminal app when its configuration says so with `runs = "terminal"` under `[app]`: `fsr dev` draws it in the terminal and `fsr build` lowers it with no browser half. `fsr new --terminal` writes that line and an Ink page. `fsr migrate` writes it for an Ink application. A root page that draws with `@snapfire/fsr/terminal/ink` in an app without it is refused with the line named.

```toml
# config/app.toml
[app]
runs = "terminal"
```

```sh
cargo install --path fsr/cli --features terminal
cd fsr/examples/counter_terminal_ts
fsr dev app
```

The command lowers the app's root page `routes/page.tsx` and draws it in the terminal it was started from. Nothing is served and no JavaScript engine starts. Ctrl-C quits, as does a page that calls `exit()`. `fsr dev` redraws the page on every edit; `fsr run app` runs it once with no watcher, the way someone using the tool runs it. What follows `--` reaches the loader as `params`.

## A page is an Ink component

The page imports Ink's names from `@snapfire/fsr/terminal/ink`, FSR's own Ink. Importing them from `ink` is refused, naming that module; `fsr migrate` writes it in place of `ink`:

```tsx
// app/routes/page.tsx
import { useState } from "react";
import { Box, Text, useInput } from "@snapfire/fsr/terminal/ink";

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
import { action } from "@snapfire/fsr/client";
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

A terminal tool reads its own configuration before it draws, so a page may read the machine it runs on through `file` and `path` from `@snapfire/fsr/host` and `system` from `@snapfire/fsr/terminal`. `file` reads and writes files: `read`, `write`, `exists`, `list`, `makeDir` and `stat`, which answers `{ isFile, isDirectory, size, mtimeMs }`. `path` joins and splits POSIX paths: `join`, `resolve`, `normalize`, `dirname`, `basename`, `extname`, `relative` and `isAbsolute`. `system` reads the machine: `env(name)`, `home()`, `platform()`, `tempDir()`, `lineEnd()`, `cwd()` and `memory()`, the resident set in bytes. Each answers at once, so an `await` of one continues with its value. Ink's `useStdout().stdout` is the terminal's size, its `on("resize", f)` running `f` when the size changes, with `off` in the effect's cleanup taking it off; `useApp().exit(code)` ends the page with that exit code.

```tsx
import { file, path } from "@snapfire/fsr/host";
import { system } from "@snapfire/fsr/terminal";

const configPath = path.join(system.home(), ".config", "cards", "config.json");
const config = file.exists(configPath) ? JSON.parse(file.read(configPath)) : { deck: "default" };
```

`config` is evaluated once, when the session opens; it is not read again on each render. A missing file throws an error whose `code` is `ENOENT`, so a `try` around the read with a default in its `catch` works. A member that fails as it is awaited inside a `try` runs the `catch`, as a rejected promise does. The same members on a web page are refused, since a browser has no files: the build says which reach the page lacks. `fsr migrate` rewrites an Ink application's imports of Node's `fs`, `path` and `os` and its reads of `process` onto these members; a page that still imports one of Node's modules or reads `process` is refused.

`crypto.randomUUID()` gives a UUID. `crypto` from `@snapfire/fsr/host` gives `random(n, encoding?)` for `n` random bytes as `hex` or `base64` and `digest("sha256", encoding, ...parts)`. `performance.now()` counts milliseconds from the process start, so two reads give a duration. A `Scope` from `@snapfire/fsr/terminal` held in a module `const` scopes a value to a call: `scope.run(value, fn)` holds it while `fn` runs across its awaits; `scope.get()` reads it, `undefined` outside any `run`. Each crypto member is a member of the `crypto` extension, so an application's own Rust under `ext/` may register another implementation under the same name. `log.info(message, fields)` from `@snapfire/fsr/host`, with `trace`, `debug`, `warn`, `error` and `fatal` beside it, writes a structured line through the host's `tracing` output under the target `fsr::app`. `text.frontMatter(source, { delimiters })` from the same module reads a YAML block at the top of a text into `{ data, content }` and `text.withFrontMatter(content, data, { delimiters })` writes one.

A function that writes fields of an object it is passed changes the caller's object; a callback of an array method that changes its item is refused, as is an async function that writes a field of its parameter. `fsr migrate` rewrites a callback that only writes fields of its parameter to return the changed object, `c => ({ ...c, n: 1 })`; the function that calls it is rewritten to take what it returns.

## Processes

An effect or a handler may start a process with `system.spawn(command, args, options)`. Its output is piped to the terminal. Each listener is a continuation like a timer's callback, so it may call the component's setters. A local the listeners write keeps its value from one report to the next. Each run of the effect gets its own. [`examples/monitor_terminal_ts`](../../examples/monitor_terminal_ts/README.md) reads a command for whichever tab is chosen:

```tsx
useEffect(() => {
  let read = "";
  const child = system.spawn("sh", ["-c", TABS[chosen].command]);
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

`ms`, `stripAnsi`, `stringWidth`, `wrapAnsi` and `bigText` from `@snapfire/fsr/terminal` run natively, since a terminal tool reaches for them constantly. FSR's `Spinner` from `@snapfire/fsr/terminal/spinner` with the spinners it reads from `@snapfire/fsr/terminal/spinners` and its `TextInput` from `@snapfire/fsr/terminal/text-input` are TypeScript modules written as `ink-spinner`, `cli-spinners` and `ink-text-input` are, which `fsr migrate` writes in place of those packages, so `<Spinner />` runs on the same timers as any other component and `<TextInput />` reads keys with `useInput` as any other component does. `BigText` from `@snapfire/fsr/terminal/big-text` takes `ink-big-text`'s props, except that `font` is the JSON of a cfonts font file the application ships rather than a name, since cfonts' fonts are GPL-3.0-or-later and FSR ships none. Other npm packages are not followed; [chapter 301](301-dependencies-without-npm.md) covers how an application gets its dependencies without npm.

## Promises and handing over the terminal

`new Promise`, `Promise.resolve`, `Promise.reject`, `await`, `try`, `catch` and `finally` run as JavaScript runs them. An async function runs to its first `await`; the rest of it is a continuation the terminal runs when the promise settles. The `await` is a statement of its own, a `const`'s value or an assignment, `cached = await readConfig()`. A `for...of` over an array, a counted `for` or a `while` whose body awaits runs each pass in turn, `for (const file of files) { const text = await readFile(file, "utf8"); … }`; `continue` and `return` work at the top of the body, `if (skip) continue;` and `if (found) return name;` among them; `break` inside such a loop is refused. That holds for one an effect or a handler declares as a `const` and calls by name, `const run = async () => { … }; run();`, as for one called where it is written. A rejected promise inside a `try` runs the `catch`, then the `finally` when there is one, then what follows the `try`. One that resolves runs the rest of the `try`, then the `finally`. A throw at the end of an async function rejects its promise. In a `catch`, `error instanceof Error` is true for a thrown `Error` and for the error a native member throws, so `if (error instanceof Error && error.code === "ENOENT")` reads as in Node. Three shapes are still refused in async code: a `try` that awaits inside another `try`, a destructuring pattern as the `catch` parameter and a `return` inside a `try` whose `finally` comes after an `await`. A component that throws a pending promise under `Suspense` shows the fallback until the promise settles.

`useApp().suspendTerminal(fn)` gives up the screen while the promise `fn` returns is pending. A process spawned inside it with `stdio: "inherit"` owns the terminal, which is how an Ink tool opens an editor. [`examples/launcher_terminal_ts`](../../examples/launcher_terminal_ts/README.md) runs `$EDITOR`, `less` and a shell this way. It pipes the commands that do not need the terminal into a panel.

## Bringing an Ink application in

`fsr migrate` reads an application whose `package.json` depends on `ink` and writes it as an FSR project:

```sh
fsr migrate ./my-ink-tool ./my-fsr-tool
```

The sources under `source/` or `src/` go to `app/src/` and a top-level `test.ts` or a `*.test.*` file to `app/tests/`. An import from `ink`, `ink-spinner`, `ink-text-input`, `cli-spinners` or `ink-big-text` is written as FSR's module of it. Each font an `ink-big-text` element names, `block` when it names none, is copied from the installed `node_modules/cfonts/fonts/` to `app/fonts/` and imported; a font chosen as the page runs reads from `app/fonts/cfonts.ts`, which maps every installed font's name to its file. An application written as CommonJS, `require` and `module.exports`, is not migrated: `fsr migrate` names each CommonJS file it would have migrated, followed from the entries `package.json` names and `source/` or `src/`, with the line that shows it, and writes nothing. The entry that calls Ink's `render` is left behind, since `app/routes/page.tsx` renders the same component instead.

When `render` passed the component props, `app/routes/page.loader.ts` is written as well: the statements before `render` in the function that rendered become the loader's body and each prop becomes a field of its return, which the page gives to the component from `RootProps`. A function, class or constant the entry declares at its top level comes along when those statements read it, with whatever it reads in turn. The loader runs once when the terminal opens, with the command line after `--` as `params`, so `fsr dev app -- --limit 3` reaches it as `params.limit`.

```tsx
import Ui from "@src/ui";

export default function Page() {
  return <Ui />;
}
```

An import written through the application's own `tsconfig.json` `paths`, `~/core/init` or `@/ui/Board`, is rewritten to `@src/core/init`, FSR's alias for that directory. A file next door is reached by a relative path instead. The alias map itself is not carried over: the project's tsconfig is FSR's, which chapter [302](302-imports-and-aliases.md) describes. What the entry read from the command line and passed as props, `render(<Ui limit={cli.flags.limit} />)`, is a note naming the props; `page.tsx` renders the component without them. `ink`, `react` and the packages FSR provides are not listed in the report; the rest of `dependencies` that a kept file imports are, for `fsr add`. A package FSR has a standard member for is rewritten to the member: `matter(source, options)` from `gray-matter` becomes `text.frontMatter(source, options)` and `matter.stringify({ content }, data, options)` becomes `text.withFrontMatter(content, data, options)`, through `await import("gray-matter")` too. A file that uses `gray-matter` any other way, `matter.read(path)` for one, keeps its import and gets a note. A logger made with `pino(options)` or winston's `createLogger(options)` becomes `log`: `logger.info({ id }, "running")` is `log.info("running", { id })`, a `logger.child({ module })` merges its fields into each call, winston's `http` and `verbose` write at `debug` and `silly` at `trace`. The options are dropped with a note, since the host's `tracing` filter decides which lines are written. A logger that is exported, passed around or used for anything but a level call, `logger.flush()` for one, leaves its file as written with a note.

## How much of Ink runs

Ink's own 29 examples are the measure, each run through `fsr migrate` first as an application would be. 26 run in the terminal with no change by hand: they lower, draw their first frame and step on their timers and keys as they do under Ink. None is refused. Three are not run by design. `jest` is built on `p-queue` with `delay` and `table` on `@faker-js/faker`, npm packages the build does not follow. `router` is built on `react-router`, whose routes FSR's own routes stand in for ([chapter 100](100-routes-loaders-and-pages.md)). [The Ink corpus](../corpus/ink.md) lists every example and how it is run and says how to [run an Ink application of your own](../corpus/ink.md#running-an-ink-application-outside-the-corpus) to see its first frame or the first thing it needs. The [terminal examples](../../examples/README.md) are six applications written to exercise what the corpus does not: [counter](../../examples/counter_terminal_ts/README.md), [tasks](../../examples/tasks_terminal_ts/README.md), [log](../../examples/log_terminal_ts/README.md), [launcher](../../examples/launcher_terminal_ts/README.md), [monitor](../../examples/monitor_terminal_ts/README.md) and [pomodoro](../../examples/pomodoro_terminal_ts/README.md).

`Transform`, `measureElement` and pages other than `routes/page.tsx` are not supported yet.

## Lab

Run the pomodoro example:

```sh
cd fsr/examples/pomodoro_terminal_ts
fsr dev app --terminal
```

Press space to start it, then again to pause it. The clock stops. Now delete the `return () => clearInterval(tick);` line from `app/routes/page.tsx` and save. The page redraws with its state kept. Start and pause it again: the clock keeps counting while it says paused, because nothing stopped the interval the first run started. Start it once more and it counts two seconds for every one, since a second interval now runs beside the first. That is what Ink does with the same code; the cleanup is the only thing that stops a timer.

Put the line back, then add `measureElement` to the import from `@snapfire/fsr/terminal/ink` and call `measureElement(null)` inside the `Bar` component. The page is replaced by the reason it cannot run, naming `Bar` and `measureElement`.
