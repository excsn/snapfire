import { Badge } from "@acme/ui";
import { Stat } from "@acme/charts";

/** Components from two vendored packages, styled by classes only those packages name: `@acme/ui` ships its class list, `@acme/charts` is read from its minified module. */
export function Vendored() {
  return (
    <div className="flex flex-wrap items-center gap-6 rounded-[10px] border border-line bg-card p-4">
      <div className="flex gap-2">
        <Badge tone="violet" dot>
          From a class list
        </Badge>
        <Badge tone="teal">Lookup table</Badge>
      </div>
      <Stat label="Read from a minified module" value="72%" share={0.72} />
    </div>
  );
}
