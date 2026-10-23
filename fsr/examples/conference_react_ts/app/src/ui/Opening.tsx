import { useLinkStatus } from "@snapfire/fsr-client/react";

/** Shown beside a link's text while the navigation it started is in flight. */
export default function Opening() {
  const { pending } = useLinkStatus();
  return <span className={pending ? "opening opening-on" : "opening"}>{pending ? "opening…" : ""}</span>;
}
