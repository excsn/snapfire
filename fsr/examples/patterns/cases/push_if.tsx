export default function Page() {
  const admin = true;
  const links = [<a key="home" href="/">home</a>];
  if (admin) {
    links.push(<a key="admin" href="/admin">admin</a>);
  }
  return <nav>{links}</nav>;
}
