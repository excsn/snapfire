import { useState } from "react";

export default function Trend({ start }: { start: number }) {
  const [count, setCount] = useState(start);
  const [prevCount, setPrevCount] = useState(0);
  const [trend, setTrend] = useState<string | null>(null);
  if (prevCount !== count) {
    setPrevCount(count);
    setTrend(count > prevCount ? "increasing" : "decreasing");
  }
  return (
    <div className="trend">
      <p className="count">{count}</p>
      {trend && <p className="direction">The count is {trend}</p>}
      <button className="less" onClick={() => setCount(count - 1)}>
        less
      </button>
    </div>
  );
}
