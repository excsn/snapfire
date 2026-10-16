export default async function SlowParent({ children }) {
  await new Promise(resolve => setTimeout(resolve, 3000));
  return <div>{children}</div>;
}
