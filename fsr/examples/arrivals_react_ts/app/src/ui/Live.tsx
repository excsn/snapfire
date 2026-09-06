import { useEffect, useState } from "react";
import { live } from "@snapfire/fsr-client";

/** Follows the field over the host's event stream: every publish of `board` revalidates the route, so the tables and the panels follow the clock without a reload. */
export default function Live({ topic }: { topic: string }) {
  const [following, setFollowing] = useState(false);
  useEffect(() => {
    setFollowing(true);
    return live([topic]);
  }, [topic]);
  return (
    <span className={following ? "live on" : "live"} title={following ? `following ${topic}` : "not following"}>
      live
    </span>
  );
}
