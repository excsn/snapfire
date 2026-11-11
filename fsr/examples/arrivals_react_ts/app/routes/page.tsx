import type { RootProps } from "@generated/client";
import type { Flight } from "@generated/services";

/** How far behind its schedule a flight is expected, `+35 min`, or nothing when it is on time. */
function lateness(flight: Flight): string {
  const late = Temporal.PlainTime.from(flight.scheduled).until(flight.expected);
  return late.sign > 0 ? `+${late.total("minutes")} min` : "";
}

function Table({ heading, column, flights }: { heading: string; column: string; flights: Flight[] }) {
  return (
    <section className="table">
      <h2>{heading}</h2>
      <table className="flights">
        <thead>
          <tr>
            <th>Flight</th>
            <th>{column}</th>
            <th>Scheduled</th>
            <th>Expected</th>
            <th>Late</th>
            <th>Gate</th>
            <th>Status</th>
          </tr>
        </thead>
        <tbody>
          {flights.map((flight) => (
            <tr key={flight.flight} className={`status-${flight.code}`}>
              <td className="flight">{flight.flight}</td>
              <td>{flight.city}</td>
              <td className="time">{flight.scheduled}</td>
              <td className="time">{flight.expected}</td>
              <td className="late">{lateness(flight)}</td>
              <td className="gate">{flight.gate}</td>
              <td className="status">{flight.status}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}

export default function BoardPage({ at, arrivals, departures, next, served }: RootProps) {
  return (
    <div className="boards">
      <p className="clock">
        Field time <strong>{at}</strong>
        {next === null ? null : (
          <span className="next">
            {" "}
            Next in: {next.flight}, {next.minutes} min
          </span>
        )}
      </p>
      <Table heading="Arrivals" column="From" flights={arrivals} />
      <Table heading="Departures" column="To" flights={departures} />
      <p className="served">Served at {served} by the server's own clock</p>
    </div>
  );
}
