import { html, render } from 'lit-html';

export const toast = (message: string): void => {
  const host = document.createElement('div');
  host.className = 'sonner-toast';

  render(html`<span>${message}</span>`, host);
  document.body.append(host);
};
