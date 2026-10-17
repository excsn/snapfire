function cells(values: string[]) {
  return values.map((v) => <td key={v}>{v}</td>);
}
export default function Page() {
  return <table><tbody><tr>{cells(["a", "b"])}</tr></tbody></table>;
}
