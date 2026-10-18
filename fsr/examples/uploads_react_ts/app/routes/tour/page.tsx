import { Embed } from "@snapfire/fsr-authoring/template";
import type { TourProps } from "@generated/client";

export default function Tour({ video }: TourProps) {
  return (
    <main className="page">
      <h1>Tour</h1>
      <p className="lede">
        This page renders no form, so it reads no CSRF token and starts no session, even under{" "}
        <code>csrf = "always"</code>. The video id comes from the loader.
      </p>
      <Embed provider="youtube" id={video.id} title={video.title} />
      <p>
        <a href="/">Back to the uploads</a>
      </p>
    </main>
  );
}
