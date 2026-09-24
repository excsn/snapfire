import { Link, type Children } from "@snapfire/fsr-authoring/template";

export default function Layout({ children }: { children: Children }) {
  return (
    <div className="{{scope}}">
      <header className="bar">
        <Link href="{{home}}" className="brand">
          {{name}}
        </Link>
      </header>
      <main className="content">{children}</main>
    </div>
  );
}
