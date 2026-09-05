import type { AccountProps } from "@generated/client";

export default function AccountPage({ subject, role }: AccountProps) {
  return (
    <div className="page account">
      <h1>Your account</h1>
      <dl className="facts">
        <dt>Signed in as</dt>
        <dd className="subject">{subject}</dd>
        <dt>Role</dt>
        <dd className="role">{role}</dd>
      </dl>
    </div>
  );
}
