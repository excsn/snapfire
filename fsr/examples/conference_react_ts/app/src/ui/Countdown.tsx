/** How far the viewer is from a talk's start and when it starts in their own zone, by their own clock, so the browser writes both. */
export default function Countdown({ startsAt }: { startsAt: string }) {
  const left = Temporal.Now.instant().until(Temporal.ZonedDateTime.from(startsAt).toInstant(), { largestUnit: "hours" });
  const days = Math.round(left.total("days"));
  const local = new Date(Temporal.ZonedDateTime.from(startsAt).epochMilliseconds).toTimeString();
  return (
    <>
      <span className="countdown">{days < 0 ? `${-days} days ago` : `in ${days} days`}</span>, <span className="your-time">at {local} your time</span>
    </>
  );
}
