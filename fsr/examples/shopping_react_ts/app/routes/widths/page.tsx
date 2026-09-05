import type { WidthsProps } from "@generated/client";

export default function Widths({ ledger, digits, lossy }: WidthsProps) {
  return (
    <div className="page widths">
      <h1>Widths survive the wire</h1>
      <p className="lede">
        The shopping service publishes two counters past 2<sup>53</sup>. The contract declares them <code>int64</code>, so they reach this page as <code>bigint</code> with every digit
        intact. The same digits through a JSON number are a different value.
      </p>
      <table className="widths-table">
        <thead>
          <tr>
            <th>field</th>
            <th>as the contract carries it</th>
            <th>as a JSON number</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td>sequence</td>
            <td className="exact">{digits}</td>
            <td className="lossy">{String(lossy)}</td>
          </tr>
          <tr>
            <td>issued</td>
            <td className="exact">{String(ledger.issued)}</td>
            <td className="lossy">{String(Number(String(ledger.issued)))}</td>
          </tr>
        </tbody>
      </table>
      <p className="quiet">{ledger.note}. The right column is what a hand-written fetch and <code>JSON.parse</code> would have shown.</p>
    </div>
  );
}
