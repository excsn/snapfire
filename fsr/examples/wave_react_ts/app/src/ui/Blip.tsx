import type { ReactElement, ReactNode } from "react";
import { Island } from "@snapfire/fsr-client/react";

import type { Blip as Kept, Part as Piece } from "@generated/client";
import Body from "@src/ui/Body";
import Under from "@src/ui/Under";

/** One blip and everything under it, rendered on the server. Its body is the parts the service parsed, walked by `Parts` and `Part`; a reply anchored to a block is a `Blip` again, placed under that block by `Aside`. The live pieces are islands: the blip's `Body`, its `Under` and an `Under` beside each block a reply can answer. */
export default function Blip({ wave, blip, me }: { wave: string; blip: Kept; me: string }): ReactElement {
  return (
    <li className="thread">
      <div className={blip.who === me ? "blip mine" : "blip"}>
        <span className="who">{blip.who}</span>
        <span className="at">{blip.at}</span>
        <div className="body md">
          <Parts wave={wave} blip={blip.id} parts={blip.parts} me={me} />
        </div>
        <Island when="load">
          <Body wave={wave} blip={blip.id} text={blip.body} edited={blip.edited} editors={blip.editors} me={me} />
        </Island>
      </div>
      <Island when="load">
        <Under wave={wave} parent={blip.id} me={me} />
      </Island>
      {blip.replies.length > 0 ? (
        <ol className="replies">
          {blip.replies.map((reply) => (
            <Blip key={reply.id} wave={wave} blip={reply} me={me} />
          ))}
        </ol>
      ) : null}
    </li>
  );
}

function Parts({ wave, blip, parts, me }: { wave: string; blip: string; parts: Piece[]; me: string }): ReactElement {
  return (
    <>
      {parts.map((part, i) => (
        <Part key={i} wave={wave} blip={blip} part={part} me={me} />
      ))}
    </>
  );
}

/** The replies anchored to a block and an `Under` of its own, which shows who is typing beside the block and offers a reply to it. It mounts once the block is in view. */
function Aside({ wave, blip, part, me }: { wave: string; blip: string; part: Piece; me: string }): ReactElement {
  return (
    <>
      {part.replies.length > 0 ? (
        <ol className="asides">
          {part.replies.map((reply) => (
            <Blip key={reply.id} wave={wave} blip={reply} me={me} />
          ))}
        </ol>
      ) : null}
      <Island when="visible">
        <Under wave={wave} parent={blip} anchor={part.at} me={me} />
      </Island>
    </>
  );
}

/** One part as the element its kind names. A paragraph, a heading or a code block sits in a `.block` with its aside after it, since a reply cannot go inside a `<p>`; a list item a reply can answer holds its aside itself. */
function Part({ wave, blip, part, me }: { wave: string; blip: string; part: Piece; me: string }): ReactNode {
  return part.kind === "text" ? (
    part.text
  ) : part.kind === "br" ? (
    <br />
  ) : part.kind === "hr" ? (
    <hr />
  ) : part.kind === "code" ? (
    <code>{part.text}</code>
  ) : part.kind === "img" ? (
    <img src={part.href} alt={part.text} />
  ) : part.kind === "a" ? (
    <a href={part.href}>
      <Parts wave={wave} blip={blip} parts={part.children} me={me} />
    </a>
  ) : part.kind === "em" ? (
    <em>
      <Parts wave={wave} blip={blip} parts={part.children} me={me} />
    </em>
  ) : part.kind === "strong" ? (
    <strong>
      <Parts wave={wave} blip={blip} parts={part.children} me={me} />
    </strong>
  ) : part.kind === "span" ? (
    <span>
      <Parts wave={wave} blip={blip} parts={part.children} me={me} />
    </span>
  ) : part.kind === "ul" ? (
    <ul>
      <Parts wave={wave} blip={blip} parts={part.children} me={me} />
    </ul>
  ) : part.kind === "ol" ? (
    <ol>
      <Parts wave={wave} blip={blip} parts={part.children} me={me} />
    </ol>
  ) : part.kind === "blockquote" ? (
    <blockquote>
      <Parts wave={wave} blip={blip} parts={part.children} me={me} />
    </blockquote>
  ) : part.kind === "li" ? (
    <li>
      <Parts wave={wave} blip={blip} parts={part.children} me={me} />
      {part.at ? <Aside wave={wave} blip={blip} part={part} me={me} /> : null}
    </li>
  ) : (
    <div className="block">
      {part.kind === "pre" ? (
        <pre>
          <code>{part.text}</code>
        </pre>
      ) : part.kind === "h1" ? (
        <h1>
          <Parts wave={wave} blip={blip} parts={part.children} me={me} />
        </h1>
      ) : part.kind === "h2" ? (
        <h2>
          <Parts wave={wave} blip={blip} parts={part.children} me={me} />
        </h2>
      ) : part.kind === "h3" ? (
        <h3>
          <Parts wave={wave} blip={blip} parts={part.children} me={me} />
        </h3>
      ) : part.kind === "h4" ? (
        <h4>
          <Parts wave={wave} blip={blip} parts={part.children} me={me} />
        </h4>
      ) : part.kind === "h5" ? (
        <h5>
          <Parts wave={wave} blip={blip} parts={part.children} me={me} />
        </h5>
      ) : part.kind === "h6" ? (
        <h6>
          <Parts wave={wave} blip={blip} parts={part.children} me={me} />
        </h6>
      ) : (
        <p>
          <Parts wave={wave} blip={blip} parts={part.children} me={me} />
        </p>
      )}
      <Aside wave={wave} blip={blip} part={part} me={me} />
    </div>
  );
}
