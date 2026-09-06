import { useEffect, useState } from "react";
import { live } from "@snapfire/fsr-client";

/** Follows this room's topic. The server publishes `room/<id>` when anyone says anything, and the route revalidates, so a message someone else sent lands here without a reload. */
export default function Follow({ room }: { room: string }) {
  const [following, setFollowing] = useState(false);
  useEffect(() => {
    setFollowing(true);
    return live([`room/${room}`]);
  }, [room]);
  return (
    <span className={following ? "live on" : "live"} title={following ? `following room/${room}` : "not following"}>
      live
    </span>
  );
}
