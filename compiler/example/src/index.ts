import { something } from './utils'; // No .js extension
export const hello = (name: string) => `Hello ${name}`;

document.getElementById('btn-default').addEventListener('click', () => {
  console.log('Default button was clicked!'); // This should be stripped
  console.debug('Some debug info.'); // This should also be stripped
  something();
});