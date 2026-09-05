import type { ReactNode } from "react";
import { Slot } from "@snapfire/fsr-client/react";

import type { AgentsLayoutProps } from "@generated/client";
import { AgentRows } from "@src/ui/AgentRows";
import { RegionBar } from "@src/ui/RegionBar";

export default function AgentsLayout({ region, agents, watching, children }: AgentsLayoutProps & { children: ReactNode }) {
  return (
    <section className="agents">
      <RegionBar region={region} shown={BigInt(agents.length)} />
      <div className="agents-body">
        <div className="agents-list">
          <AgentRows agents={agents} watching={watching} />
        </div>
        <div className="agents-detail">{children}</div>
        <div className="agents-peek">
          <Slot name="peek">
            <p className="peek-hint">Peek at an agent without leaving the list.</p>
          </Slot>
        </div>
      </div>
    </section>
  );
}
