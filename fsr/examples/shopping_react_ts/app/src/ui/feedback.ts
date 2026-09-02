import Swal from "sweetalert2";
import { ActionFailure } from "@snapfire/fsr-client";

import { money } from "./money";

const toast = Swal.mixin({ toast: true, position: "top-end", showConfirmButton: false, timer: 1800, timerProgressBar: true });

export function addedToCart(name: string, cartCount: bigint | number): void {
  void toast.fire({ icon: "success", title: "Added to your cart", text: `${name}. ${Number(cartCount)} in the cart.` });
}

export function removedFromCart(name: string): void {
  void toast.fire({ icon: "info", title: "Removed", text: name });
}

export function failed(error: unknown): void {
  const text = error instanceof ActionFailure ? error.message : error instanceof Error ? error.message : String(error);
  void Swal.fire({ icon: "error", title: "That did not work", text });
}

export async function confirmOrder(items: number, totalCents: number): Promise<boolean> {
  const result = await Swal.fire({
    icon: "question",
    title: "Place your order?",
    text: `${items} item${items === 1 ? "" : "s"}, ${money(totalCents)} in total.`,
    showCancelButton: true,
    confirmButtonText: "Place order",
    cancelButtonText: "Keep shopping",
    confirmButtonColor: "#ffa41c",
  });
  return result.isConfirmed;
}

export function orderPlaced(id: bigint | number, totalCents: bigint | number, lines: number): void {
  void Swal.fire({
    icon: "success",
    title: `Order #${String(id)} placed`,
    html: `<p>${lines} line${lines === 1 ? "" : "s"}, ${money(totalCents)} charged.</p><p>Thank you for shopping with us.</p>`,
    confirmButtonText: "Back to the catalog",
    confirmButtonColor: "#ffa41c",
  });
}
