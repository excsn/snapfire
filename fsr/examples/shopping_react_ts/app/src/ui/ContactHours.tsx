import { useState } from "react";

export function ContactHours() {
  const [weekend, setWeekend] = useState(false);
  return (
    <li className="contact-hours">
      {weekend ? "Saturday 10 to 2" : "Weekdays 9 to 5"}{" "}
      <button className="link" onClick={() => setWeekend(!weekend)}>
        {weekend ? "Weekday hours" : "Weekend hours"}
      </button>
    </li>
  );
}
