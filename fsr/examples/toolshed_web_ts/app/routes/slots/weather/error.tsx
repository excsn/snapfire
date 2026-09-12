export default function WeatherError() {
  return (
    <div className="panel weather panel-down">
      <h2>Workday weather</h2>
      <p className="quiet">The forecast is not answering, as expected: the mock declares this call a failure. Everything else on this page is unaffected.</p>
    </div>
  );
}
