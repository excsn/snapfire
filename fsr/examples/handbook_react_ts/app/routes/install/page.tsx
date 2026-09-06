import type { InstallProps } from "@generated/client";

export default function InstallPage({ steps }: InstallProps) {
  return (
    <div className="page install">
      <h1>Install</h1>
      <ol className="steps">
        {steps.map((step) => (
          <li key={step.command}>
            <code>{step.command}</code>
            <span>{step.explains}</span>
          </li>
        ))}
      </ol>
    </div>
  );
}
