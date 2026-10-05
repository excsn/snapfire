import { Island, Link } from "@snapfire/fsr-authoring/template";

import CtxAction from "@src/ui/CtxAction";
import CtxStep from "@src/ui/CtxStep";

export default function CtxPage({ id, q, path, locale, subject, note, stamp }: { id: string; q: string; path: string; locale: string; subject: string; note: string; stamp: string }) {
  return (
    <section className="ctx">
      <dl className="loaded">
        <dt>id</dt>
        <dd className="id">{id}</dd>
        <dt>q</dt>
        <dd className="q">{q}</dd>
        <dt>path</dt>
        <dd className="path">{path}</dd>
        <dt>locale</dt>
        <dd className="locale">{locale}</dd>
        <dt>subject</dt>
        <dd className="subject">{subject}</dd>
        <dt>note</dt>
        <dd className="note">{note}</dd>
        <dt>stamp</dt>
        <dd className="stamp">{stamp}</dd>
      </dl>
      <Link href="/ctx/b?q=2" className="other">
        other
      </Link>
      <Island>
        <CtxAction />
      </Island>
      <Island mode="server">
        <CtxStep />
      </Island>
    </section>
  );
}
