import { useState } from "react";

export default function MountStamp({ when }: { when: string }) {
  const [at] = useState(() => Math.round(performance.now()));
  return (
    <p className="stamp">
      <span className="badge">island hydrated {when}</span>
      <span className="stamp-at">
        mounted <b>{at}ms</b> after navigation start
      </span>
    </p>
  );
}
