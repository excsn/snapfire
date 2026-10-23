import { use, useState } from "react";

let finish: () => void = () => {};
const exported = new Promise<void>((resolve) => (finish = resolve));

(globalThis as { finishPlannerExport?: () => void }).finishPlannerExport = () => finish();

export default function Export() {
  const [started, setStarted] = useState(false);
  if (started) use(exported);
  return <button className="export" onClick={() => setStarted(true)}>{started ? "Exported" : "Export"}</button>;
}
