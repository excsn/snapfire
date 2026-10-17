export default function Page() {
  const open = [1, 2].length > 1;
  let body;
  if (open) {
    body = <div className="open">open</div>;
  } else {
    body = <div className="closed">closed</div>;
  }
  return <main>{body}</main>;
}
