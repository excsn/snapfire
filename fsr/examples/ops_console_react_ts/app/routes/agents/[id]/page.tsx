import { get, optimistic } from "@snapfire/fsr-client";
import { Island } from "@snapfire/fsr-client/react";
import { t } from "@snapfire/fsr-client/std";

import { actions, type AgentsIdProps } from "@generated/client";
import { JobTimeline } from "@src/ui/JobTimeline";
import { watching } from "@src/store";

export default function AgentPage({ agent, jobs, watched }: AgentsIdProps) {
  async function watch(): Promise<void> {
    await optimistic(watching, (get(watching) ?? 0) + 1, () => actions.agents.watchAgent({ agent_id: agent.id }));
  }

  async function unwatch(): Promise<void> {
    await optimistic(watching, (get(watching) ?? 1) - 1, () => actions.settings.unwatchAgent({ agent_id: agent.id }));
  }

  return (
    <article className="page agent">
      <h1>{agent.name}</h1>
      <p className="lede">
        {t("agent.lede", { region: agent.region, status: agent.status, queued: String(agent.queue_depth), cpu: agent.cpu.toFixed(1) })}
      </p>
      {watched ? (
        <button className="btn" onClick={() => void unwatch()}>{t("agent.unwatch")}</button>
      ) : (
        <button className="btn" onClick={() => void watch()}>{t("agent.watch")}</button>
      )}
      <h2>{t("agent.running")}</h2>
      <Island when="visible">
        <JobTimeline jobs={jobs} />
      </Island>
    </article>
  );
}
