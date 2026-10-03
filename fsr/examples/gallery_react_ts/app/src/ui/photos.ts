import type { ImageAsset } from "@snapfire/fsr-client/react";

import harbour from "../img/harbour.jpg";
import ridge from "../img/ridge.jpg";
import dunes from "../img/dunes.jpg";
import marsh from "../img/marsh.jpg";

/** Every photo the wall can show, by the file name a loader hands over. */
export const PHOTOS: Record<string, ImageAsset> = { "harbour.jpg": harbour, "ridge.jpg": ridge, "dunes.jpg": dunes, "marsh.jpg": marsh };
