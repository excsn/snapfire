import { t } from "@snapfire/fsr-client/std";

import type { AgentsIdProps } from "@generated/client";

export default function AgentPeek({ agent, jobs }: AgentsIdProps) {
  return (
    <div className="peek">
      <h3>{agent.name}</h3>
      <p>
        {t("agent.peek", { queued: String(agent.queue_depth), cpu: agent.cpu.toFixed(1) })}
      </p>
      <ul className="peek-jobs">
        {jobs.map((j) => (
          <li key={String(j.id)}>{j.name}</li>
        ))}
      </ul>
    </div>
  );
}
