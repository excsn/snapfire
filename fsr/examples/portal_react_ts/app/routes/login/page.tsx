import type { LoginProps } from "@generated/client";

const field = "grid gap-1 text-[13px] text-ink-soft";
const input = "rounded-md border border-line bg-card px-2.5 py-1.5 text-[15px] text-ink";

export default function LoginPage({ denied }: LoginProps) {
  return (
    <div>
      <h1>Sign in</h1>
      <p className="mb-5 text-ink-soft">
        Dev accounts: <code>alice</code> / <code>wonder</code> or <code>bob</code> / <code>builder</code>. Signing in here signs you in on every mounted site.
      </p>
      {denied ? <p className="text-bad" data-field="denied">Unknown user or wrong password.</p> : null}
      <form method="post" action="/auth/callback" className="grid max-w-80 gap-3">
        <label className={field}>
          User
          <input name="user" className={input} />
        </label>
        <label className={field}>
          Password
          <input name="password" type="password" className={input} />
        </label>
        <button className="button justify-self-start">Sign in</button>
      </form>
    </div>
  );
}
