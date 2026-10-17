function Card({ header, footer }: { header: any; footer?: any }) {
  return <section>{header}<hr />{footer}</section>;
}
export default function Page() {
  return <Card header={<h2>Head</h2>} footer={<small>foot</small>} />;
}
