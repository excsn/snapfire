import type { AgentsViewProps } from "@generated/client";

export default function AgentPeek({ agent, jobs }: AgentsViewProps) {
  return (
    <div className="peek">
      <h3>{agent.name}</h3>
      <p>
        {String(agent.queue_depth)} queued · {agent.cpu.toFixed(1)}% cpu
      </p>
      <ul className="peek-jobs">
        {jobs.map((j) => (
          <li key={String(j.id)}>{j.name}</li>
        ))}
      </ul>
    </div>
  );
}
