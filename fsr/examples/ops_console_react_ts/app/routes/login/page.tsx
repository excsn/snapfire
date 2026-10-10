import { t } from "@snapfire/fsr-client/std";

import type { LoginProps } from "@generated/client";

export default function LoginPage({ denied }: LoginProps) {
  return (
    <div className="page login">
      <h1>{t("login.title")}</h1>
      <p className="lede">
        {t("login.accounts")} <code>alice</code> / <code>wonder</code> {t("login.or")} <code>bob</code> / <code>builder</code>.
      </p>
      {denied ? <p className="denied">{t("login.denied")}</p> : null}
      <form method="post" action="/auth/callback" className="signin">
        <label>
          {t("login.user")}
          <input name="user" />
        </label>
        <label>
          {t("login.password")}
          <input name="password" type="password" />
        </label>
        <button>{t("login.submit")}</button>
      </form>
    </div>
  );
}
