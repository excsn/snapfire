import { Badge } from "@acme/ui";
import { Stat } from "@acme/charts";

export function App() {
  return (
    <main className="p-4">
      <Badge>new</Badge>
      <Stat value={3} />
    </main>
  );
}
