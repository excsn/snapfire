import { useState } from "react";
function Toggle({ label }: { label: any }) {
  const [on, setOn] = useState(false);
  return <button onClick={() => setOn(!on)}>{label}{on ? "on" : "off"}</button>;
}
export default function Page() {
  return <Toggle label={<b>Light</b>} />;
}
