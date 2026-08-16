export class Widget {
  private #hidden = 1;

  reveal(): number {
    return this.#hidden;
  }
}
