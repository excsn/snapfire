import { system } from "@snapfire/fsr-client/std";
import { useEffect, useRef, useState } from "react";
import { Box, Text, useAnimation, useBoxMetrics, useInput, useWindowSize } from "ink";
import { stripAnsi } from "@snapfire/fsr/terminal";

const spinner = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

function Clock() {
  const [now, setNow] = useState(() => new Date().toLocaleTimeString());
  useEffect(() => {
    const timer = setInterval(() => setNow(new Date().toLocaleTimeString()), 1000);
    return () => clearInterval(timer);
  }, []);
  return <Text dimColor>{now}</Text>;
}

export default function Log() {
  const [lines, setLines] = useState<string[]>([]);
  const [status, setStatus] = useState("loading");
  const [top, setTop] = useState(0);
  const { frame } = useAnimation({ interval: 80, isActive: status === "loading" });
  const view = useRef(null);
  const content = useRef(null);
  const { clientHeight } = useBoxMetrics(view);
  const { height } = useBoxMetrics(content);
  const { columns } = useWindowSize();
  const last = Math.max(0, height - clientHeight);
  const shown = Math.min(top, last);

  useEffect(() => {
    const git = system.spawn("git", ["log", "--oneline", "--color=always", "-n", "200"]);
    git.stdout.setEncoding("utf8");
    git.stdout.on("data", (chunk: string) => {
      setLines((held) => [...held, ...stripAnsi(chunk).split("\n").filter((line) => line !== "")]);
    });
    git.on("exit", (code) => setStatus(code === 0 ? "done" : `git exited with ${code}`));
    git.on("error", (error) => setStatus(error.message));
  }, []);

  useInput((input, key) => {
    if (key.downArrow || input === "j") setTop(Math.min(last, shown + 1));
    if (key.upArrow || input === "k") setTop(Math.max(0, shown - 1));
    if (input === "g") setTop(0);
    if (input === "G") setTop(last);
  });

  return (
    <Box flexDirection="column" width={columns}>
      <Box justifyContent="space-between">
        <Text bold>
          {status === "loading" ? `${spinner[frame % spinner.length]} reading git log` : status === "done" ? `${lines.length} commits` : status}
        </Text>
        <Clock />
      </Box>
      <Box ref={view} height={12} overflow="hidden" contentOffsetY={shown} borderStyle="round" flexDirection="column">
        <Box ref={content} flexDirection="column" flexShrink={0}>
          {lines.map((line, index) => (
            <Text key={index} wrap="truncate">
              <Text color="yellow">{line.slice(0, line.indexOf(" "))}</Text>
              {line.slice(line.indexOf(" "))}
            </Text>
          ))}
        </Box>
      </Box>
      <Text dimColor>
        {last > 0 ? `${shown + 1}-${Math.min(lines.length, shown + clientHeight)} of ${lines.length}` : ""} j and k or the arrows scroll, g and G jump, Ctrl-C quits
      </Text>
    </Box>
  );
}
