import { get, optimistic } from "@snapfire/fsr-client";
import { useStore } from "@snapfire/fsr-client/react";

import { actions, type Agent } from "@generated/client";
import { density, watching } from "@src/store";

export function SettingsPanel({ watched }: { watched: Agent[] }) {
  const [rows] = useStore(density, "comfortable");

  async function unwatch(id: bigint | number): Promise<void> {
    await optimistic(watching, (get(watching) ?? 1) - 1, () => actions.settings.unwatchAgent({ agent_id: id }));
  }

  async function choose(next: string): Promise<void> {
    await optimistic(density, next, () => actions.settings.setDensity({ density: next }));
  }

  return (
    <div className="settings">
      <section>
        <h2>Rows</h2>
        <div className="segmented" role="radiogroup" aria-label="Row density">
          <button className={rows === "comfortable" ? "seg seg-on" : "seg"} onClick={() => void choose("comfortable")}>
            Comfortable
          </button>
          <button className={rows === "compact" ? "seg seg-on" : "seg"} onClick={() => void choose("compact")}>
            Compact
          </button>
        </div>
      </section>
      <section>
        <h2>Watching</h2>
        {watched.length === 0 ? <p className="quiet">Nothing yet. Watch an agent from the list.</p> : null}
        <ul className="watch-list">
          {watched.map((a) => (
            <li key={String(a.id)}>
              <span className="agent-name">{a.name}</span>
              <span className="agent-region">{a.region}</span>
              <button className="btn btn-small" onClick={() => void unwatch(a.id)}>
                unwatch
              </button>
            </li>
          ))}
        </ul>
      </section>
    </div>
  );
}
