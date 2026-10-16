export function App() {
  return (
    <my-element
      onspeak={e => console.log(e.detail.message)}
    ></my-element>
  )
}
