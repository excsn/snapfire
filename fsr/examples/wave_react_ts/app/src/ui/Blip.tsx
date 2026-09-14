import type { ReactElement, ReactNode } from "react";
import { Island } from "@snapfire/fsr-client/react";

import type { Blip as Kept, Block as Source, Part as Piece } from "@generated/client";
import Block from "@src/ui/Block";
import Body from "@src/ui/Body";
import Gadget from "@src/ui/Gadget";
import Under from "@src/ui/Under";

/** One blip and everything under it, rendered on the server. Its body is its blocks, each the parts the service parsed, walked by `Parts` and `Part`; a reply anchored to a block is a `Blip` again, placed under that block by `Aside`. The live pieces are islands: the blip's `Body`, its `Under`, a `Block` around each block and an `Under` beside each block a reply can answer. A step of playback is not `live`, so it has none of them and nothing on it can be written. */
export default function Blip({ wave, blip, me, live }: { wave: string; blip: Kept; me: string; live: boolean }): ReactElement {
  return (
    <li className="thread">
      <div id={`blip-${blip.id}`} className={`${blip.who === me ? "blip mine" : "blip"}${blip.lit ? " lit" : ""}`}>
        <span className="who">{blip.who}</span>
        <span className="at">{blip.at}</span>
        <div className="body md">
          {blip.blocks.map((block) => (
            <Section key={block.id} wave={wave} blip={blip.id} block={block} me={me} mine={blip.who === me} live={live} />
          ))}
        </div>
        {live ? (
          <Island when="load">
            <Body wave={wave} blip={blip.id} text={blip.body} edited={blip.edited} editors={blip.editors} me={me} />
          </Island>
        ) : (
          <Stamp edited={blip.edited} editors={blip.editors} />
        )}
      </div>
      {live ? (
        <Island when="load">
          <Under wave={wave} parent={blip.id} me={me} />
        </Island>
      ) : null}
      {blip.replies.length > 0 ? (
        <ol className="replies">
          {blip.replies.map((reply) => (
            <Blip key={reply.id} wave={wave} blip={reply} me={me} live={live} />
          ))}
        </ol>
      ) : null}
    </li>
  );
}

/** Who has rewritten a blip and when it was last, as `Body` shows them, for a step of playback. */
function Stamp({ edited, editors }: { edited: string; editors: string[] }): ReactNode {
  return edited ? (
    <div className="body-foot">
      <ul className="editors">
        {editors.map((who) => (
          <li key={who} className="editor">
            {who}
          </li>
        ))}
      </ul>
      <span className="edited">edited {edited}</span>
    </div>
  ) : null;
}

/** One block inside a `Block` island, whose children are its parts rendered here on the server. A paragraph, a heading or a code block keeps its aside outside the island, so its replies and its composer stay put while the block is rewritten; a list or a quote holds the asides of its items. A gadget block is its gadget, a server-mode island, with its aside after it. A step of playback has no browser islands, so any other block is its parts. */
function Section({ wave, blip, block, me, mine, live }: { wave: string; blip: string; block: Source; me: string; mine: boolean; live: boolean }): ReactElement {
  return block.gadget.kind !== "" ? (
    <div className="block">
      <Island mode="server">
        <Gadget wave={wave} blip={blip} block={block.id} gadget={block.gadget} live={live} mine={mine} />
      </Island>
      <Aside wave={wave} blip={blip} part={block.parts[0]} me={me} live={live} />
    </div>
  ) : !live ? (
    <Parts wave={wave} blip={blip} parts={block.parts} me={me} live={live} />
  ) : block.parts.length === 1 && block.parts[0].at !== "" ? (
    <div className="block">
      <Island when="visible">
        <Block wave={wave} blip={blip} block={block.id} text={block.text} me={me}>
          <Leaf wave={wave} blip={blip} part={block.parts[0]} me={me} live={live} />
        </Block>
      </Island>
      <Aside wave={wave} blip={blip} part={block.parts[0]} me={me} live={live} />
    </div>
  ) : (
    <Island when="visible">
      <Block wave={wave} blip={blip} block={block.id} text={block.text} me={me}>
        <Parts wave={wave} blip={blip} parts={block.parts} me={me} live={live} />
      </Block>
    </Island>
  );
}

function Parts({ wave, blip, parts, me, live }: { wave: string; blip: string; parts: Piece[]; me: string; live: boolean }): ReactElement {
  return (
    <>
      {parts.map((part, i) => (
        <Part key={i} wave={wave} blip={blip} part={part} me={me} live={live} />
      ))}
    </>
  );
}

/** The replies anchored to a block and an `Under` of its own, which shows who is typing beside the block and offers a reply to it. It mounts once the block is in view. */
function Aside({ wave, blip, part, me, live }: { wave: string; blip: string; part: Piece; me: string; live: boolean }): ReactElement {
  return (
    <>
      {part.replies.length > 0 ? (
        <ol className="asides">
          {part.replies.map((reply) => (
            <Blip key={reply.id} wave={wave} blip={reply} me={me} live={live} />
          ))}
        </ol>
      ) : null}
      {live ? (
        <Island when="visible">
          <Under wave={wave} parent={blip} anchor={part.at} me={me} />
        </Island>
      ) : null}
    </>
  );
}

/** A paragraph, a heading or a code block as its element, without the aside a `.block` puts after it. */
function Leaf({ wave, blip, part, me, live }: { wave: string; blip: string; part: Piece; me: string; live: boolean }): ReactElement {
  return part.kind === "pre" ? (
    <pre>
      <code>{part.text}</code>
    </pre>
  ) : part.kind === "h1" ? (
    <h1>
      <Parts wave={wave} blip={blip} parts={part.children} me={me} live={live} />
    </h1>
  ) : part.kind === "h2" ? (
    <h2>
      <Parts wave={wave} blip={blip} parts={part.children} me={me} live={live} />
    </h2>
  ) : part.kind === "h3" ? (
    <h3>
      <Parts wave={wave} blip={blip} parts={part.children} me={me} live={live} />
    </h3>
  ) : part.kind === "h4" ? (
    <h4>
      <Parts wave={wave} blip={blip} parts={part.children} me={me} live={live} />
    </h4>
  ) : part.kind === "h5" ? (
    <h5>
      <Parts wave={wave} blip={blip} parts={part.children} me={me} live={live} />
    </h5>
  ) : part.kind === "h6" ? (
    <h6>
      <Parts wave={wave} blip={blip} parts={part.children} me={me} live={live} />
    </h6>
  ) : (
    <p>
      <Parts wave={wave} blip={blip} parts={part.children} me={me} live={live} />
    </p>
  );
}

/** One part as the element its kind names. A paragraph, a heading or a code block sits in a `.block` with its aside after it, since a reply cannot go inside a `<p>`; a list item a reply can answer holds its aside itself. */
function Part({ wave, blip, part, me, live }: { wave: string; blip: string; part: Piece; me: string; live: boolean }): ReactNode {
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
      <Parts wave={wave} blip={blip} parts={part.children} me={me} live={live} />
    </a>
  ) : part.kind === "em" ? (
    <em>
      <Parts wave={wave} blip={blip} parts={part.children} me={me} live={live} />
    </em>
  ) : part.kind === "strong" ? (
    <strong>
      <Parts wave={wave} blip={blip} parts={part.children} me={me} live={live} />
    </strong>
  ) : part.kind === "span" ? (
    <span>
      <Parts wave={wave} blip={blip} parts={part.children} me={me} live={live} />
    </span>
  ) : part.kind === "ul" ? (
    <ul>
      <Parts wave={wave} blip={blip} parts={part.children} me={me} live={live} />
    </ul>
  ) : part.kind === "ol" ? (
    <ol>
      <Parts wave={wave} blip={blip} parts={part.children} me={me} live={live} />
    </ol>
  ) : part.kind === "blockquote" ? (
    <blockquote>
      <Parts wave={wave} blip={blip} parts={part.children} me={me} live={live} />
    </blockquote>
  ) : part.kind === "li" ? (
    <li>
      <Parts wave={wave} blip={blip} parts={part.children} me={me} live={live} />
      {part.at ? <Aside wave={wave} blip={blip} part={part} me={me} live={live} /> : null}
    </li>
  ) : (
    <div className="block">
      <Leaf wave={wave} blip={blip} part={part} me={me} live={live} />
      <Aside wave={wave} blip={blip} part={part} me={me} live={live} />
    </div>
  );
}
