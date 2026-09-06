import type { Block, Run } from "@src/docs/guide";

export function Runs({ runs }: { runs: Run[] }) {
  return (
    <>
      {runs.map((r, i) =>
        r.kind === "code" ? (
          <code key={String(i)}>{r.text}</code>
        ) : r.kind === "strong" ? (
          <strong key={String(i)}>{r.text}</strong>
        ) : r.kind === "em" ? (
          <em key={String(i)}>{r.text}</em>
        ) : r.kind === "link" ? (
          <a key={String(i)} href={r.href}>
            {r.text}
          </a>
        ) : (
          <span key={String(i)}>{r.text}</span>
        ),
      )}
    </>
  );
}

export function Prose({ blocks }: { blocks: Block[] }) {
  return (
    <div className="prose">
      {blocks.map((b, i) =>
        b.kind === "heading" ? (
          b.level === 2 ? (
            <h2 key={String(i)}>
              <Runs runs={b.runs} />
            </h2>
          ) : (
            <h3 key={String(i)}>
              <Runs runs={b.runs} />
            </h3>
          )
        ) : b.kind === "code" ? (
          <div className="code-card" key={String(i)}>
            <div className="code-card-header">
              <span>{b.lang === "" ? "text" : b.lang}</span>
            </div>
            <pre>
              <code>{b.code}</code>
            </pre>
          </div>
        ) : b.kind === "list" ? (
          <ul className="prose-list" key={String(i)}>
            {b.items.map((item, j) => (
              <li key={String(j)}>
                <Runs runs={item} />
              </li>
            ))}
          </ul>
        ) : b.kind === "table" ? (
          <div className="table-wrap" key={String(i)}>
            <table className="prose-table">
              <thead>
                <tr>
                  {b.items.map((cell, j) => (
                    <th key={String(j)}>
                      <Runs runs={cell} />
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {b.rows.map((row, j) => (
                  <tr key={String(j)}>
                    {row.map((cell, k) => (
                      <td key={String(k)}>
                        <Runs runs={cell} />
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : b.kind === "quote" ? (
          <blockquote key={String(i)}>
            <Runs runs={b.runs} />
          </blockquote>
        ) : (
          <p key={String(i)}>
            <Runs runs={b.runs} />
          </p>
        ),
      )}
    </div>
  );
}
