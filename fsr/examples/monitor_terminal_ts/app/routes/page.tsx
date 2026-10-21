import { useEffect, useRef, useState } from "react";
import { spawn } from "node:child_process";
import stripAnsi from "strip-ansi";
import { Box, Text, useAnimation, useApp, useBoxMetrics, useInput, useWindowSize } from "ink";

const TABS = [
  { name: "Load", command: "uptime" },
  { name: "Disk", command: "df -h" },
  { name: "Processes", command: "ps -Ao pid,pcpu,comm -r" },
];

const SPINNER = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

function Spinner({ label }: { label: string }) {
  const { frame } = useAnimation({ interval: 80 });
  return <Text color="yellow">{SPINNER[frame % SPINNER.length]} {label}</Text>;
}

function TabBar({ chosen }: { chosen: number }) {
  return (
    <Box gap={2}>
      {TABS.map((tab, index) => (
        <Text key={tab.name} bold={index === chosen} inverse={index === chosen}>
          {` ${index + 1} ${tab.name} `}
        </Text>
      ))}
    </Box>
  );
}

export default function Monitor() {
  const { exit } = useApp();
  const { columns } = useWindowSize();
  const panel = useRef(null);
  const { width, hasMeasured } = useBoxMetrics(panel);
  const [chosen, setChosen] = useState(0);
  const [output, setOutput] = useState("");
  const [running, setRunning] = useState(true);
  const [failure, setFailure] = useState("");
  const [runs, setRuns] = useState(0);

  useEffect(() => {
    setRunning(true);
    setFailure("");
    let read = "";
    const child = spawn("sh", ["-c", TABS[chosen].command]);
    child.stdout.on("data", (chunk) => {
      read = read + chunk;
    });
    child.on("exit", (code) => {
      setRunning(false);
      setRuns((n) => n + 1);
      if (code === 0) setOutput(stripAnsi(read));
      else setFailure(`${TABS[chosen].command} exited with ${code}`);
    });
  }, [chosen]);

  useInput((input, key) => {
    if (input === "q") exit();
    else if (key.rightArrow || input === "l") setChosen((chosen + 1) % TABS.length);
    else if (key.leftArrow || input === "h") setChosen((chosen + TABS.length - 1) % TABS.length);
    else if (input === "1" || input === "2" || input === "3") setChosen(Number(input) - 1);
  });

  const narrow = columns < 60;
  const shown = output.split("\n").filter((line) => line.length > 0).slice(0, 12);

  return (
    <Box flexDirection="column">
      <Box justifyContent="space-between">
        <Text bold>Monitor</Text>
        <Text dimColor>{columns} columns</Text>
      </Box>
      <TabBar chosen={chosen} />
      <Box flexDirection={narrow ? "column" : "row"}>
        <Box ref={panel} width={narrow ? "100%" : "70%"} flexDirection="column" borderStyle="round" paddingX={1}>
          {running ? <Spinner label={`running ${TABS[chosen].command}`} /> : null}
          {failure ? <Text color="red">{failure}</Text> : shown.map((line, index) => <Text key={index} wrap="truncate">{line}</Text>)}
        </Box>
        <Box width={narrow ? "100%" : "30%"} flexDirection="column" borderStyle="round" paddingX={1}>
          <Text bold>{TABS[chosen].name}</Text>
          <Text dimColor>{TABS[chosen].command}</Text>
          <Text>{runs} {runs === 1 ? "run" : "runs"}</Text>
        </Box>
      </Box>
      <Text dimColor>
        ←/→ or 1-3 switch, q quits{hasMeasured ? `, panel ${width} wide` : ""}
      </Text>
    </Box>
  );
}
