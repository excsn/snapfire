import { Badge } from "./badge.js";

export function Card({ name }: { name: string }) {
  return (
    <div>
      <Badge label={name} />
    </div>
  );
}
