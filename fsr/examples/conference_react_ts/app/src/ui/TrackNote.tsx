/** Registered by hand in `main.ts`, so it mounts with no patcher. */
export default function TrackNote({ track }: { track: string }) {
  return <p className="track-note">Showing {track}</p>;
}
