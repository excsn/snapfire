function Component() {
  return <button onClick={() => console.error('Some console error')}>Trigger console.error()</button>;
}

export default function App() {
  return <Component />;
}
