import { get, optimistic, set } from "@snapfire/fsr-client";
import { useStore } from "@snapfire/fsr-client/react";
import { Link } from "@snapfire/fsr/components";
import { t } from "@snapfire/fsr-client/std";

import { queueLabel } from "@ext/fleet";
import { actions, type Agent } from "@generated/client";
import { density, region, selected, watching } from "@src/store";

export function AgentRows({ agents, watching: held }: { agents: Agent[]; watching: string[] }) {
  const [chosen] = useStore(selected, "");
  const [shown] = useStore(region, "all");
  const [rows] = useStore(density, "comfortable");
  const tail = shown === "all" ? "" : `?region=${shown}`;

  async function watch(id: bigint | number): Promise<void> {
    await optimistic(watching, (get(watching) ?? 0) + 1, () => actions.agents.watchAgent({ agent_id: id }));
  }

  async function unwatch(id: bigint | number): Promise<void> {
    await optimistic(watching, (get(watching) ?? 1) - 1, () => actions.settings.unwatchAgent({ agent_id: id }));
  }

  return (
    <ul className={rows === "compact" ? "agent-rows agent-rows-compact" : "agent-rows"}>
      {agents.map((a) => (
        <li key={String(a.id)} className={chosen === String(a.id) ? "agent-row agent-row-on" : "agent-row"}>
          <span className={a.status === "up" ? "dot dot-up" : "dot dot-down"} title={a.status} />
          <Link href={`/agents/${a.id}${tail}`} full className="agent-name" onClick={() => set(selected, String(a.id))}>
            {a.name}
          </Link>
          <span className="agent-region">{a.region}</span>
          <span className="agent-queue">{queueLabel(Number(a.queue_depth))}</span>
          <Link href={`/agents/${a.id}${tail}`} into="peek" className="btn btn-small">
            {t("agents.peek")}
          </Link>
          {held.includes(String(a.id)) ? (
            <button className="btn btn-small" onClick={() => void unwatch(a.id)}>
              {t("agents.unwatch")}
            </button>
          ) : (
            <button className="btn btn-small" onClick={() => void watch(a.id)}>
              {t("agents.watch")}
            </button>
          )}
        </li>
      ))}
    </ul>
  );
}
