import { useState } from "react";

export function useCounter(start: number, step = 1) {
  const [count, setCount] = useState(start);
  const doubled = count * 2;
  function bump() {
    setCount(count + step);
  }
  return { count, doubled, bump, reset: setCount };
}

export function useToggle(initial: boolean) {
  const [on, setOn] = useState(initial);
  return [on, () => setOn(!on)] as const;
}
