import type { RootProps } from "@generated/client";
import type { Flight } from "@generated/services";

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
              <td className="gate">{flight.gate}</td>
              <td className="status">{flight.status}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}

export default function BoardPage({ at, arrivals, departures }: RootProps) {
  return (
    <div className="boards">
      <p className="clock">
        Field time <strong>{at}</strong>
      </p>
      <Table heading="Arrivals" column="From" flights={arrivals} />
      <Table heading="Departures" column="To" flights={departures} />
    </div>
  );
}
