import type { ImageAsset } from "@snapfire/fsr-client/react";

import p1 from "../img/products/1.png";
import p2 from "../img/products/2.png";
import p3 from "../img/products/3.png";
import p4 from "../img/products/4.png";
import p5 from "../img/products/5.png";
import p6 from "../img/products/6.png";
import p7 from "../img/products/7.png";
import p8 from "../img/products/8.png";
import p9 from "../img/products/9.png";
import p10 from "../img/products/10.png";
import p11 from "../img/products/11.png";
import p12 from "../img/products/12.png";
import p13 from "../img/products/13.png";
import p14 from "../img/products/14.png";

/** Every product photo by the file the catalog names, so a `Picture` lowers one branch per photo, the browser picks the same one and a loader's `meta` preloads the one its page shows. */
export const PHOTOS: { [file: string]: ImageAsset } = {
  "1.png": p1,
  "2.png": p2,
  "3.png": p3,
  "4.png": p4,
  "5.png": p5,
  "6.png": p6,
  "7.png": p7,
  "8.png": p8,
  "9.png": p9,
  "10.png": p10,
  "11.png": p11,
  "12.png": p12,
  "13.png": p13,
  "14.png": p14,
};
