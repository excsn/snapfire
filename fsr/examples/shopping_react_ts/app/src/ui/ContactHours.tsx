import { useContext, useState } from "react";

import { OrderNumber } from "@src/ui/order";

export function ContactHours() {
  const [weekend, setWeekend] = useState(false);
  const order = useContext(OrderNumber);
  return (
    <li className="contact-hours">
      {weekend ? "Saturday 10 to 2" : "Weekdays 9 to 5"}{" "}
      <button className="link" onClick={() => setWeekend(!weekend)}>
        {weekend ? "Weekday hours" : "Weekend hours"}
      </button>
      {order === "" ? null : <span className="quote"> and quote #{order}</span>}
    </li>
  );
}
