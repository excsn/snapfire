import { useLocale } from "@snapfire/fsr-client/react";

export default function Help() {
  const locale = useLocale();
  return locale === "fr_FR" ? (
    <div className="page help">
      <h1>Comment ça marche</h1>
      <p>
        Chaque panneau de cette page est rendu par le serveur. Le nombre d'alertes dans l'en-tête, la région dans la barre et le nombre d'agents suivis sont un seul store partagé par
        tout le document, alimenté par les loaders et écrit par le panneau qui les change.
      </p>
      <p>Le navigateur ne charge rien ici. Les panneaux arrivent en un seul arbre rendu par le serveur, et les parties plus lentes que les autres arrivent en flux derrière leurs propres attentes.</p>
    </div>
  ) : (
    <div className="page help">
      <h1>How this works</h1>
      <p>
        Every panel on this page is rendered by the server. The alert count in the header, the region in the bar and the number of agents you watch are one store the whole document
        shares, seeded by the loaders and written by whichever panel changes them.
      </p>
      <p>Nothing here is fetched by the browser. The panels arrive as one server-rendered tree, and the parts that are slower than the rest stream in behind their own placeholders.</p>
    </div>
  );
}
