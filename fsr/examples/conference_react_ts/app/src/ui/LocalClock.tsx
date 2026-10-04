export default function LocalClock() {
  const zone = typeof Intl === "undefined" ? "an unknown zone" : Intl.DateTimeFormat().resolvedOptions().timeZone;
  return <p className="local-clock">Times are the venue's. You are in {zone}.</p>;
}
