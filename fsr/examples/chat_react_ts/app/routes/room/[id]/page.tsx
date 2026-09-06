import type { RoomIdProps } from "@generated/client";

import Say from "@src/ui/Say";
import Follow from "@src/ui/Follow";

export default function RoomPage({ room, messages, me }: RoomIdProps) {
  return (
    <div className="page room">
      <div className="room-head">
        <h1>{room.name}</h1>
        <Follow room={room.id} />
      </div>
      <p className="about">{room.about}</p>
      <ol className="transcript">
        {messages.map((message) => (
          <li key={message.id} className={message.who === me ? "said mine" : "said"}>
            <span className="at">{message.at}</span>
            <span className="who">{message.who}</span>
            <span className="body">{message.body}</span>
          </li>
        ))}
      </ol>
      <Say room={room.id} />
    </div>
  );
}
