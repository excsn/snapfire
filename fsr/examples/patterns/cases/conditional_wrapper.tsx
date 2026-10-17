function Wrap({ link, children }: { link?: string; children?: any }) {
  const inner = <span>{children}</span>;
  return link ? <a href={link}>{inner}</a> : inner;
}
export default function Page() {
  return <div><Wrap link="/x">linked</Wrap><Wrap>plain</Wrap></div>;
}
