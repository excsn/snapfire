import type { AccountProps } from "@generated/client";

export default function AccountPage({ subject, role }: AccountProps) {
  return (
    <div>
      <h1>Your account</h1>
      <dl className="grid grid-cols-[max-content_1fr] gap-x-4 gap-y-1.5 [&_dt]:text-ink-soft">
        <dt>Signed in as</dt>
        <dd data-field="subject">{subject}</dd>
        <dt>Role</dt>
        <dd data-field="role">{role}</dd>
      </dl>
    </div>
  );
}
