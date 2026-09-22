import { action } from "@snapfire/fsr";
import type { ActionCtx } from "@snapfire/fsr";
import type { Deposit } from "@schemas/upload";

const ACCEPTED = ["image/png", "image/jpeg", "image/webp", "text/plain"];

export const deposit = action(async ({ input, session }: ActionCtx<Deposit>) => {
  if (!ACCEPTED.includes(input.file.content_type)) {
    session.error = `${input.file.content_type} is not one this example takes`;
    return { ok: false };
  }
  if (input.file.size === 0n) {
    session.error = "that file is empty";
    return { ok: false };
  }

  session.error = "";
  session.held = [
    ...session.held,
    {
      filename: input.file.filename,
      content_type: input.file.content_type,
      size: input.file.size,
      caption: input.caption,
    },
  ];
  return { ok: true };
});

export const clear = action(async ({ session }: ActionCtx) => {
  session.held = [];
  session.error = "";
  return { ok: true };
});
