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

/** The one connection a wave needs, shared by every island on the page: the socket that carries drafts and presence, and the `live` stream that says when a blip was kept. Each island that joins holds a share; the last to leave closes it. */
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

/** Says what this reader is part way through typing, or nothing when the field is empty. */
export function typing(parent: string, body: string): void {
  held?.socket.send("typing", { parent, body });
}
