import type { Job } from "@generated/client";

export function JobTimeline({ jobs }: { jobs: Job[] }) {
  const longest = jobs.reduce((n, j) => (Number(j.seconds) > n ? Number(j.seconds) : n), 1);

  return (
    <ul className="timeline">
      {jobs.map((j) => (
        <li key={String(j.id)}>
          <span className="timeline-name">{j.name}</span>
          <span className="timeline-bar" style={{ width: `${Math.round((Number(j.seconds) / longest) * 100)}%` }} />
          <span className="timeline-secs">{String(j.seconds)}s</span>
        </li>
      ))}
    </ul>
  );
}
