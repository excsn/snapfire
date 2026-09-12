import type { LayoutWeatherProps } from "@generated/client";

export default function Weather({ weather }: LayoutWeatherProps) {
  return (
    <div className="panel weather">
      <h2>Workday weather</h2>
      <p>
        <span className="text">{weather.day}</span> <span className="at">{weather.summary}</span>
      </p>
    </div>
  );
}
