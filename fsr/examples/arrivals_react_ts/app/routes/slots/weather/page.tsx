import type { LayoutWeatherProps } from "@generated/client";

export default function Weather({ weather }: LayoutWeatherProps) {
  return (
    <div className="panel weather">
      <h2>The field</h2>
      <p className="reading">{weather.field}</p>
      <dl>
        <div>
          <dt>Wind</dt>
          <dd>{weather.wind}</dd>
        </div>
        <div>
          <dt>Visibility</dt>
          <dd>{weather.visibility}</dd>
        </div>
        <div>
          <dt>Temperature</dt>
          <dd>{weather.celsius}°C</dd>
        </div>
      </dl>
    </div>
  );
}
