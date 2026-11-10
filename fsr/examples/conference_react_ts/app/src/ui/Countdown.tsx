/** How far the viewer is from a talk's start, by their own clock, so the browser writes it. */
export default function Countdown({ startsAt }: { startsAt: string }) {
  const left = Temporal.Now.instant().until(Temporal.ZonedDateTime.from(startsAt).toInstant(), { largestUnit: "hours" });
  const days = Math.round(left.total("days"));
  return <span className="countdown">{days < 0 ? `${-days} days ago` : `in ${days} days`}</span>;
}
