export default function Page() {
  const stars = [];
  for (let i = 0; i < 3; i++) {
    stars.push(<span key={i}>*</span>);
  }
  return <p>{stars}</p>;
}
