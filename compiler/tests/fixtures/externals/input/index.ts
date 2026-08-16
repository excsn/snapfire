import { html } from 'lit';
import { debounce } from 'lodash/debounce';
import { thing } from '@scope/pkg';
import { cdn } from 'https://cdn.example.com/mod.js';
import { rooted } from '/assets/vendor.js';
import { local } from './helper';

export const all = [html, debounce, thing, cdn, rooted, local];
