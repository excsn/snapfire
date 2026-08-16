import { a } from './deep/a';
import { html } from 'lit';

export const lazy = () => import('./deferred');
export const all = [a, html];
