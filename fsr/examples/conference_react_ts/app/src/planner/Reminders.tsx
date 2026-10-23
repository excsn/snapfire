import { useState } from "react";

export default function Reminders() {
  const [asked, setAsked] = useState(false);
  if (asked) throw new Error("reminders are unavailable");
  return <button className="reminders" onClick={() => setAsked(true)}>Remind me</button>;
}
