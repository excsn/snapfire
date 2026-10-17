function Star() { return <b>*</b>; }
function Button({ icon: Icon, label }: { icon: () => any; label: string }) {
  return <button><Icon />{label}</button>;
}
export default function Page() {
  return <Button icon={Star} label="fav" />;
}
