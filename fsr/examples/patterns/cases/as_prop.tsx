function Box({ as: Tag = "div", children }: { as?: any; children?: any }) {
  return <Tag className="box">{children}</Tag>;
}
export default function Page() {
  return <Box as="section">inside</Box>;
}
