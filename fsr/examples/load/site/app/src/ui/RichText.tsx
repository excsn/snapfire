import type { RichNode } from "./types";

function Node({ node }: { node: RichNode }) {
  if (node.type === "text") {
    if (node.bold) {
      return <strong>{node.text}</strong>;
    }
    if (node.italic) {
      return <em>{node.text}</em>;
    }
    return <>{node.text}</>;
  }
  if (node.type === "linebreak") {
    return <br />;
  }
  if (node.type === "link") {
    return node.external ? (
      <a href={node.href} target="_blank" rel="noopener noreferrer">
        {node.children.map((child, index) => (
          <Node key={index} node={child} />
        ))}
      </a>
    ) : (
      <a href={node.href}>
        {node.children.map((child, index) => (
          <Node key={index} node={child} />
        ))}
      </a>
    );
  }
  if (node.type === "list") {
    const items = node.children.map((child, index) => <Node key={index} node={child} />);
    return node.ordered ? <ol>{items}</ol> : <ul>{items}</ul>;
  }
  if (node.type === "item") {
    return (
      <li>
        {node.children.map((child, index) => (
          <Node key={index} node={child} />
        ))}
      </li>
    );
  }
  return (
    <p>
      {node.children.map((child, index) => (
        <Node key={index} node={child} />
      ))}
    </p>
  );
}

export default function RichText({ nodes }: { nodes: RichNode[] }) {
  return (
    <div className="rich">
      {nodes.map((node, index) => (
        <Node key={index} node={node} />
      ))}
    </div>
  );
}
