import { live, socket, type Socket } from "@snapfire/fsr-client";

interface Wire {
  topic: string;
  socket: Socket;
  stop: () => void;
  users: number;
}

let held: Wire | null = null;
const watching = new Set<(open: boolean) => void>();

function tell(open: boolean): void {
  for (const watcher of watching) watcher(open);
}

/** The one connection a wave needs, shared by every island on the page: the socket that carries drafts and presence, plus the `live` stream that says when a blip was kept. Each island that joins holds a share; the last to leave closes it. */
export function join(topic: string, onOpen: (open: boolean) => void): () => void {
  watching.add(onOpen);
  if (!held || held.topic !== topic) {
    held?.socket.close();
    held?.stop();
    held = {
      topic,
      socket: socket(topic, { onOpen: () => tell(true), onClose: () => tell(false) }),
      stop: live([topic]),
      users: 0,
    };
  }
  held.users += 1;
  onOpen(held.socket.open());
  return () => {
    watching.delete(onOpen);
    if (!held) return;
    held.users -= 1;
    if (held.users <= 0) {
      held.socket.close();
      held.stop();
      held = null;
    }
  };
}

/** Says this reader is part way through typing under `parent` or has stopped when `body` is empty. The words go only when `shown`; otherwise the others are told who is typing and nothing of what. `anchor` is the block it answers, empty for the whole blip. */
export function typing(parent: string, anchor: string, body: string, shown = false): void {
  held?.socket.send("typing", { parent, anchor, writing: body !== "", body: shown ? body : "" });
}

/** Says the reader has named themselves, so presence stops calling them nobody. The socket joined before the name existed, so the name travels as a row rather than in the handshake. */
export function named(name: string): void {
  held?.socket.send("named", { name });
}

/** Reaches for one block of a blip to rewrite it; an empty `block` reaches for the whole blip. The field decides: whoever gets there first holds it and the other window learns so by being sent the holder's text. */
export function hold(blip: string, block: string): void {
  held?.socket.send("open", { blip, block });
}

/** The rewrite as it stands, one row per keystroke, kept by nobody until the action runs. */
export function rewriting(blip: string, block: string, body: string): void {
  held?.socket.send("rewriting", { blip, block, body });
}

/** Lets a block or a blip go without keeping the rewrite. */
export function release(blip: string, block: string): void {
  held?.socket.send("close", { blip, block });
}
