import type { AccountProps } from "@generated/client";

const themes = [
  { value: "", label: "Each page's own" },
  { value: "default", label: "Default" },
  { value: "ocean", label: "Ocean" },
  { value: "forest", label: "Forest" },
];

const schemes = [
  { value: "", label: "Match system" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

const field = "grid gap-1 text-[13px] text-ink-soft";
const select = "rounded-md border border-line bg-card px-2.5 py-1.5 text-[15px] text-ink";

export default function AccountPage({ subject, role, theme, scheme, csrf_token }: AccountProps) {
  return (
    <div className="grid gap-8">
      <section>
        <h1>Your account</h1>
        <dl className="grid grid-cols-[max-content_1fr] gap-x-4 gap-y-1.5 [&_dt]:text-ink-soft">
          <dt>Signed in as</dt>
          <dd data-field="subject">{subject}</dd>
          <dt>Role</dt>
          <dd data-field="role">{role}</dd>
        </dl>
      </section>
      <section className="rounded-[10px] border border-line bg-card p-5">
        <h2 className="mb-1 text-lg font-semibold">Look</h2>
        <p className="mb-4 text-ink-soft">Your pick follows you onto every site the portal mounts, except billing, which forces its own theme. The scheme holds everywhere.</p>
        <form method="post" action="/_sf/action/account.setLook" className="flex flex-wrap items-end gap-4">
          <input type="hidden" name="_csrf" value={csrf_token ?? ""} />
          <label className={field}>
            Theme
            <select name="theme" className={select}>
              {themes.map((option) => (
                <option key={option.value} value={option.value} selected={option.value === theme}>
                  {option.label}
                </option>
              ))}
            </select>
          </label>
          <label className={field}>
            Scheme
            <select name="scheme" className={select}>
              {schemes.map((option) => (
                <option key={option.value} value={option.value} selected={option.value === scheme}>
                  {option.label}
                </option>
              ))}
            </select>
          </label>
          <button className="button">Save</button>
        </form>
      </section>
    </div>
  );
}
