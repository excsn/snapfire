import type { RootProps } from "@generated/client";

export default function Index({ systems, summary }: RootProps) {
  return (
    <div className="status-page">
      <h1>Status</h1>
      <p className="status-summary">{summary}</p>
      <table className="status-table">
        <thead>
          <tr>
            <th>System</th>
            <th>Owner</th>
            <th>State</th>
            <th>Note</th>
          </tr>
        </thead>
        <tbody>
          {systems.map((system) => (
            <tr key={system.name}>
              <td>{system.name}</td>
              <td>{system.owner}</td>
              <td>
                <span className={`status-state status-${system.state}`}>{system.state}</span>
              </td>
              <td>{system.note}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
