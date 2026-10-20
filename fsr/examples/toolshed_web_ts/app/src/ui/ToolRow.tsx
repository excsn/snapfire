import { use } from "@snapfire/fsr-authoring/template";
import { Link } from "@snapfire/fsr/components";

import type { Tool } from "@generated/client";
import { Reserved } from "@src/reserved";

export function ToolRow({ tool }: { tool: Tool }) {
  return (
    <li className="tool-row">
      <span className="what">
        <Link href={`/tool/${tool.id}`} className="tool-title">
          {tool.name}
        </Link>
        <span className="by">
          {tool.keeper}'s, £{tool.deposit} deposit, {tool.days} days
        </span>
      </span>
      <span className={`category category-${tool.category.toLowerCase()}`}>{tool.category}</span>
      {use(Reserved).includes(tool.id) ? <span className="kept">reserved</span> : <span className="kept kept-off" />}
    </li>
  );
}
