import type { LayoutAnnouncementsProps } from "@generated/client";

export default function Announcements({ notices }: LayoutAnnouncementsProps) {
  return (
    <div className="panel announcements">
      <h2>From the desk</h2>
      {notices.length === 0 ? <p className="quiet">Nothing yet.</p> : null}
      <ul>
        {notices.map((notice) => (
          <li key={notice.at}>
            <span className="at">{notice.at}</span>
            <span className="text">{notice.text}</span>
          </li>
        ))}
      </ul>
      <p className="note">Read every time. No cache policy on this method.</p>
    </div>
  );
}
