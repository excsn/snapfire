export class MyElement extends HTMLElement {
  constructor() {
    super();
    // The value here will be overwritten by React
    // when initialized as an element
    this.value = undefined;
  }

  connectedCallback() {
    this.innerHTML = this.value.join(", ");
  }
}
