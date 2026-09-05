import type { LoginProps } from "@generated/client";

export default function LoginPage({ denied }: LoginProps) {
  return (
    <div className="page login">
      <h1>Sign in</h1>
      <p className="lede">
        Dev accounts: <code>alice</code> / <code>wonder</code> or <code>bob</code> / <code>builder</code>.
      </p>
      {denied ? <p className="denied">Unknown user or wrong password.</p> : null}
      <form method="post" action="/auth/callback" className="signin">
        <label>
          User
          <input name="user" />
        </label>
        <label>
          Password
          <input name="password" type="password" />
        </label>
        <button>Sign in</button>
      </form>
    </div>
  );
}
