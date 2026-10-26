export default function NoticesPage({ notices }: { notices: string[] }) {
  return (
    <section className="page">
      <ul className="venue-notices">
        {notices.map((notice) => (
          <li key={notice}>{notice}</li>
        ))}
      </ul>
    </section>
  );
}
