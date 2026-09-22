import { useRef, useState } from "react";
import { upload } from "@snapfire/fsr-client";
import { Island } from "@snapfire/fsr-client/react";
import type { RootProps } from "@generated/client";

export default function Page({ held, error, csrf_token }: RootProps & { csrf_token?: string }) {
  return (
    <main className="page">
      <h1>Uploads</h1>
      <p className="lede">
        One action, two callers. The form below posts natively, so it works with JavaScript switched off. The panel
        under it posts the same action as a <code>FormData</code>, which is the same wire with a different caller.
      </p>

      {error ? <p className="error">{error}</p> : null}

      <section className="panel">
        <h2>Without JavaScript</h2>
        <form method="post" action="/_sf/action/$root.deposit" encType="multipart/form-data">
          <input type="hidden" name="_csrf" value={csrf_token ?? ""} />
          <label>
            Caption
            <input name="caption" defaultValue="" />
          </label>
          <label>
            File
            <input type="file" name="file" />
          </label>
          <button type="submit">Deposit</button>
        </form>
      </section>

      <section className="panel">
        <h2>With it</h2>
        <Island when="load">
          <Deposit token={csrf_token ?? ""} />
        </Island>
      </section>

      <section className="panel">
        <h2>Held this session</h2>
        {held.length === 0 ? (
          <p className="empty">Nothing yet.</p>
        ) : (
          <table className="held">
            <thead>
              <tr>
                <th>File</th>
                <th>Type</th>
                <th>Bytes</th>
                <th>Caption</th>
              </tr>
            </thead>
            <tbody>
              {held.map((row) => (
                <tr key={`${row.filename}-${String(row.size)}`}>
                  <td>{row.filename}</td>
                  <td>{row.content_type}</td>
                  <td>{String(row.size)}</td>
                  <td>{row.caption}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        <form method="post" action="/_sf/action/$root.clear">
          <input type="hidden" name="_csrf" value={csrf_token ?? ""} />
          <button type="submit">Clear</button>
        </form>
      </section>
    </main>
  );
}

export function Deposit({ token }: { token: string }) {
  const form = useRef<HTMLFormElement>(null);
  const [state, setState] = useState("");

  async function send(event: React.FormEvent) {
    event.preventDefault();
    if (!form.current) return;
    const data = new FormData(form.current);
    data.set("_csrf", token);
    setState("posting");
    try {
      await upload("$root.deposit", data);
      setState("done");
    } catch (e) {
      setState(e instanceof Error ? e.message : "failed");
    }
  }

  return (
    <form ref={form} onSubmit={send}>
      <label>
        Caption
        <input name="caption" defaultValue="" />
      </label>
      <label>
        File
        <input type="file" name="file" />
      </label>
      <button type="submit">Deposit</button>
      {state ? <span className="state">{state}</span> : null}
    </form>
  );
}
