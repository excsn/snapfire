import { Link } from "@snapfire/fsr/components";
import type { RootProps } from "@generated/client";

export default function Index({ trails }: RootProps) {
  return (
    <ul className="list">
      {trails.map((t) => (
        <li key={t.id}>
          <Link href={`/trail/${t.id}`}>{t.name}</Link>
          <span>
            {t.region}, {t.km} km
          </span>
        </li>
      ))}
    </ul>
  );
}
