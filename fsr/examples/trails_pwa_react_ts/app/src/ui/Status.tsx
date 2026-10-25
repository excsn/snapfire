import { online, updateReady } from "@snapfire/fsr/client";
import { useStore } from "@snapfire/fsr/client/react";
import { Link } from "@snapfire/fsr/components";

export default function Status() {
  const [connected] = useStore(online, true);
  const [ready] = useStore(updateReady, false);
  return (
    <div className="status">
      {connected ? null : <p className="offline">Offline: you are reading saved pages.</p>}
      {ready ? (
        <p className="update">
          A new version is ready.{" "}
          <Link href="/" native>
            Open it
          </Link>
        </p>
      ) : null}
    </div>
  );
}
