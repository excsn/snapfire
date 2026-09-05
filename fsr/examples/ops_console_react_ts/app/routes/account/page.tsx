import type { AccountProps } from "@generated/client";

export default function AccountPage({ subject, role, agents }: AccountProps) {
  return (
    <div className="page account">
      <h1>Your account</h1>
      <p className="lede">The loader read the identity the host holds for this session; the fleet call it made carried your token.</p>
      <dl className="facts">
        <dt>Signed in as</dt>
        <dd className="subject">{subject}</dd>
        <dt>Role</dt>
        <dd className="role">{role}</dd>
        <dt>Agents visible</dt>
        <dd>{Number(agents)}</dd>
      </dl>
    </div>
  );
}
