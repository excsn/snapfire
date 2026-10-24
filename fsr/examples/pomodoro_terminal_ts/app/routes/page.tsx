import { useEffect, useState } from "react";
import { ms } from "@snapfire/fsr/terminal";
import { Box, Text, useApp, useInput, useStdout } from "@snapfire/fsr/terminal/ink";

const WORK = 25 * 60;
const BREAK = 5 * 60;

function clock(seconds: number) {
  const minutes = Math.floor(seconds / 60);
  const rest = seconds % 60;
  return `${String(minutes).padStart(2, "0")}:${String(rest).padStart(2, "0")}`;
}

function Bar({ done, total, width }: { done: number; total: number; width: number }) {
  const filled = Math.round((done / total) * width);
  return (
    <Text>
      <Text color="green">{"█".repeat(filled)}</Text>
      <Text dimColor>{"░".repeat(width - filled)}</Text>
    </Text>
  );
}

export default function Pomodoro() {
  const { exit } = useApp();
  const { write } = useStdout();
  const [working, setWorking] = useState(true);
  const [left, setLeft] = useState(WORK);
  const [running, setRunning] = useState(false);
  const [rounds, setRounds] = useState(0);

  useEffect(() => {
    if (!running) return;
    const tick = setInterval(() => {
      setLeft((left) => left - 1);
    }, 1000);
    return () => clearInterval(tick);
  }, [running]);

  useEffect(() => {
    if (left > 0) return;
    write(`${working ? "work" : "break"} finished after ${ms((working ? WORK : BREAK) * 1000, { long: true })}\n`);
    if (working) setRounds((n) => n + 1);
    setWorking(!working);
    setLeft(working ? BREAK : WORK);
  }, [left]);

  useInput((input) => {
    if (input === "q") exit();
    else if (input === " ") setRunning(!running);
    else if (input === "r") {
      setRunning(false);
      setLeft(working ? WORK : BREAK);
    } else if (input === "s") setLeft(0);
  });

  const total = working ? WORK : BREAK;

  return (
    <Box flexDirection="column" borderStyle="round" paddingX={1} width={40}>
      <Box justifyContent="space-between">
        <Text bold color={working ? "red" : "cyan"}>{working ? "Work" : "Break"}</Text>
        <Text>{rounds} {rounds === 1 ? "round" : "rounds"}</Text>
      </Box>
      <Text bold>{clock(left)}{running ? "" : " paused"}</Text>
      <Bar done={total - left} total={total} width={36} />
      <Text dimColor>space starts or pauses, r resets, s skips, q quits</Text>
    </Box>
  );
}
