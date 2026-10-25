import { useState } from "react";
import { Box, Text, useApp, useInput } from "@snapfire/fsr/terminal/ink";

export default function Page() {
  const [count, setCount] = useState(0);
  const { exit } = useApp();

  useInput((input, key) => {
    if (input === "+" || key.upArrow) setCount(count + 1);
    if (input === "-" || key.downArrow) setCount(count - 1);
    if (input === "q") exit();
  });

  return (
    <Box flexDirection="column" borderStyle="round" paddingX={1}>
      <Text bold>{"{{name}}"}</Text>
      <Text>{count}</Text>
      <Text dimColor>+ or up, - or down, q quits</Text>
    </Box>
  );
}
