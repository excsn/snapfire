import type { Ctx } from "@snapfire/fsr";

export async function load({ session }: Ctx<"/">) {
  return {
    held: session.held,
    error: session.error,
  };
}

export const meta = () => ({
  title: "Uploads",
  description: "A file posted to an action, with and without JavaScript.",
});
