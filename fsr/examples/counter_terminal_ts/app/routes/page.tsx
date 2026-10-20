import { useState } from "react";
import { Box, Newline, Spacer, Text, useInput } from "ink";

export default function Counter() {
  const [count, setCount] = useState(0);
  const [step, setStep] = useState(1);

  useInput((input, key) => {
    if (input === "+" || key.upArrow) setCount(count + step);
    if (input === "-" || key.downArrow) setCount(count - step);
    if (key.rightArrow) setStep(step * 10);
    if (key.leftArrow && step > 1) setStep(step / 10);
    if (input === "r") setCount(0);
  });

  return (
    <Box flexDirection="column" borderStyle="round" borderColor="cyan" paddingX={1} width={40}>
      <Box>
        <Text bold color="cyan">
          Counter
        </Text>
        <Spacer />
        <Text dimColor>step {step}</Text>
      </Box>
      <Box marginY={1} justifyContent="center">
        <Text bold color={count < 0 ? "red" : "green"}>
          {count}
        </Text>
      </Box>
      <Text dimColor>
        + or up, - or down
        <Newline />
        right and left change the step, r resets
        <Newline />
        Ctrl-C quits
      </Text>
    </Box>
  );
}
