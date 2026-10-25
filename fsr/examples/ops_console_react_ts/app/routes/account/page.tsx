import { t } from "@snapfire/fsr/client";

import type { AccountProps } from "@generated/client";

export default function AccountPage({ subject, role, agents }: AccountProps) {
  return (
    <div className="page account">
      <h1>{t("account.title")}</h1>
      <p className="lede">{t("account.lede")}</p>
      <dl className="facts">
        <dt>{t("account.signed_in")}</dt>
        <dd className="subject">{subject}</dd>
        <dt>{t("account.role")}</dt>
        <dd className="role">{role}</dd>
        <dt>{t("account.agents")}</dt>
        <dd>{Number(agents)}</dd>
      </dl>
    </div>
  );
}
