export function TipList() {
  return (
    <details className="tips">
      <summary>What to try</summary>
      <ul>
        <li>Click an agent: its page renders under the list, which stays as it is.</li>
        <li>Click peek: the same route renders into the panel beside the list.</li>
        <li>Open the settings gear: the settings route renders into a drawer over the console.</li>
        <li>Open an agent from an alert: from the list it peeks, from anywhere else it navigates.</li>
        <li>Acknowledge an alert: the header count moves before the server answers.</li>
      </ul>
    </details>
  );
}
