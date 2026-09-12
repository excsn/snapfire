import type { LayoutMarketProps } from "@generated/client";

export default function Market({ stalls }: LayoutMarketProps) {
  return (
    <div className="panel market">
      <h2>At the market</h2>
      <ul>
        {stalls.map((stall) => (
          <li key={stall.name}>
            <span className="text">{stall.name}</span>
            <span className="at">{stall.has}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}
