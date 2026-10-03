import hero from "../img/hero.png";
import photo from "../img/photo.jpg";
import inter from "../fonts/inter.woff2";
import config from "./card.json" with { type: "json" };

export const picture = hero;
export const upright = photo;
export const face = inter;
export const name: string = config.name;
