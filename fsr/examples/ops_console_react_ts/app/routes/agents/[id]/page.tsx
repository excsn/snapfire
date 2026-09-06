import { get, optimistic } from "@snapfire/fsr-client";
import { Island } from "@snapfire/fsr-client/react";

import { actions, type AgentsIdProps } from "@generated/client";
import { JobTimeline } from "@src/ui/JobTimeline";
import { watching } from "@src/store";

export default function AgentPage({ agent, jobs }: AgentsIdProps) {
  async function watch(): Promise<void> {
    await optimistic(watching, (get(watching) ?? 0) + 1, () => actions.agents.watchAgent({ agent_id: agent.id }));
  }

  return (
    <article className="page agent">
      <h1>{agent.name}</h1>
      <p className="lede">
        {agent.region} · {agent.status} · {String(agent.queue_depth)} queued · {agent.cpu.toFixed(1)}% cpu
      </p>
      <button className="btn" onClick={() => void watch()}>
        Watch this agent
      </button>
      <h2>Running</h2>
      <Island when="visible">
        <JobTimeline jobs={jobs} />
      </Island>
    </article>
  );
}
