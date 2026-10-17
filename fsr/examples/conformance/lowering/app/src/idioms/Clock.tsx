const price = new Intl.NumberFormat("de-DE", { style: "currency", currency: "EUR" });

export default function Clock({ total }: { total: number }) {
  return (
    <p className="clock">
      <span className="at">{new Date().toLocaleTimeString()}</span> <span className="price">{price.format(total)}</span>{" "}
      <span className="day">{new Intl.DateTimeFormat("en-US", { weekday: "long", timeZone: "UTC" }).format(new Date("2025-06-15T12:00:00Z"))}</span>
    </p>
  );
}
