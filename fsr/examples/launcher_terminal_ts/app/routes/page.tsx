import { system } from "@snapfire/fsr/terminal";
import { Suspense, useState } from "react";
import { Box, Text, useApp, useInput } from "@snapfire/fsr/terminal/ink";
import { ms } from "@snapfire/fsr/terminal";

type Command = { label: string; run: string; owns: boolean };

const commands: Command[] = [
  { label: "Edit a scratch file", run: `${system.env("EDITOR") ?? "vi"} /tmp/fsr-scratch.txt`, owns: true },
  { label: "Page through the README", run: "less README.md", owns: true },
  { label: "Open a shell", run: "sh", owns: true },
  { label: "Show the date", run: "date", owns: false },
  { label: "Show the disk use", run: "df -h .", owns: false },
];

let captured = "";

/** Runs `command` with the terminal handed to it, settling when it exits. */
const attached = (command: string): Promise<void> =>
  new Promise((resolve, reject) => {
    const child = system.spawn("sh", ["-c", command], { stdio: "inherit" });
    child.on("exit", (code) => {
      if (code === 0) {
        resolve();
      } else {
        reject(new Error(`exited with ${code}`));
      }
    });
    child.on("error", reject);
  });

/** Runs `command` with its output piped, settling with what it wrote. */
const capture = (command: string): Promise<string> =>
  new Promise((resolve, reject) => {
    captured = "";
    const child = system.spawn("sh", ["-c", command]);
    child.stdout.on("data", (chunk: string) => {
      captured = captured + chunk;
    });
    child.on("exit", (code) => {
      if (code === 0) {
        resolve(captured);
      } else {
        reject(new Error(`exited with ${code}`));
      }
    });
    child.on("error", reject);
  });

let reading: Promise<void> | undefined;
let described: string | undefined;

/** What `uname -sm` says, which suspends the first render until it answers. */
const describe = (): string => {
  if (described !== undefined) {
    return described;
  }
  if (!reading) {
    reading = capture("uname -sm").then((text) => {
      described = text.trim();
    });
  }
  throw reading;
};

function System() {
  return <Text dimColor>on {describe()}</Text>;
}

export default function Launcher() {
  const { exit, suspendTerminal } = useApp();
  const [chosen, setChosen] = useState(0);
  const [status, setStatus] = useState("Enter runs the chosen command");
  const [output, setOutput] = useState("");

  useInput((input, key) => {
    if (key.upArrow) setChosen((c) => Math.max(0, c - 1));
    if (key.downArrow) setChosen((c) => Math.min(commands.length - 1, c + 1));
    if (input === "q") exit();
    if (!key.return) return;
    const command = commands[chosen];
    void (async () => {
      const started = Date.now();
      setStatus(`running ${command.label}`);
      try {
        if (command.owns) {
          await suspendTerminal(async () => {
            await attached(command.run);
          });
          setOutput("");
        } else {
          const text = await capture(command.run);
          setOutput(text.trimEnd());
        }
        setStatus(`${command.label} finished in ${ms(Date.now() - started)}`);
      } catch (error) {
        setStatus(`${command.label} failed: ${(error as Error).message}`);
      }
    })();
  });

  return (
    <Box flexDirection="column">
      <Box justifyContent="space-between">
        <Text bold>Launcher</Text>
        <Suspense fallback={<Text dimColor>reading the system</Text>}>
          <System />
        </Suspense>
      </Box>
      <Box flexDirection="column" marginY={1}>
        {commands.map((command, index) => (
          <Text key={command.label} color={index === chosen ? "cyan" : undefined}>
            {index === chosen ? "> " : "  "}
            {command.label}
            <Text dimColor> {command.run}</Text>
          </Text>
        ))}
      </Box>
      <Text>{status}</Text>
      {output === "" ? null : (
        <Box borderStyle="round" paddingX={1}>
          <Text>{output}</Text>
        </Box>
      )}
      <Text dimColor>up and down choose, Enter runs, q quits</Text>
    </Box>
  );
}
