import { t } from "@snapfire/fsr/client";

export default function AgentError() {
  return (
    <article className="page agent">
      <h1>{t("agent.error_title")}</h1>
      <p>{t("agent.error_body")}</p>
    </article>
  );
}
