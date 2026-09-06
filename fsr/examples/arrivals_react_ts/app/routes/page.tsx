import type { RootProps } from "@generated/client";

export default function BoardPage({ arrivals }: RootProps) {
  return (
    <table className="arrivals">
      <thead>
        <tr>
          <th>Flight</th>
          <th>From</th>
          <th>Due</th>
          <th>Gate</th>
          <th>Status</th>
        </tr>
      </thead>
      <tbody>
        {arrivals.map((arrival) => (
          <tr key={arrival.flight} className={`status-${arrival.code}`}>
            <td className="flight">{arrival.flight}</td>
            <td>{arrival.from}</td>
            <td>{arrival.due}</td>
            <td className="gate">{arrival.gate}</td>
            <td className="status">{arrival.status}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
