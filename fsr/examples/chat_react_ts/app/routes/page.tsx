import type { RootProps } from "@generated/client";
import { Link } from "@snapfire/fsr/components";

import Name from "@src/ui/Name";

export default function RoomsPage({ rooms, next }: RootProps) {
  return (
    <div className="page rooms">
      <Name next={next} />
      <ul className="room-list">
        {rooms.map((room) => (
          <li key={room.id}>
            <Link href={`/room/${room.id}`} className="room-card">
              <span className="room-name">{room.name}</span>
              <span className="room-about">{room.about}</span>
              <span className="room-count">{room.messages}</span>
            </Link>
          </li>
        ))}
      </ul>
    </div>
  );
}
