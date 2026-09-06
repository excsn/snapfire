import type { FsrBenchesProps } from "@generated/client";

export default function BenchesPage({ benchmarks }: FsrBenchesProps) {
  return (
    <div className="page benches">
      <h1>Renderer Benchmarks</h1>
      <p className="lede">
        Tested on Apple M4 Pro using Criterion. Measuring raw page generation time
        of lowered FSR IR against React 18 production <code>renderToString</code> inside QuickJS.
      </p>

      <table className="bench-table">
        <thead>
          <tr>
            <th>Target Page</th>
            <th>QuickJS (Cold Start)</th>
            <th>QuickJS (Warm React)</th>
            <th>FSR Rust IR</th>
            <th>Speedup</th>
          </tr>
        </thead>
        <tbody>
          {benchmarks.map((b) => (
            <tr key={b.name}>
              <td><strong>{b.name}</strong></td>
              <td className="dim">{b.cold}</td>
              <td>{b.warm}</td>
              <td className="highlight-green">{b.fsr}</td>
              <td className="highlight-accent">{b.speedup}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}