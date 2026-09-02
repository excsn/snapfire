declare module "sweetalert2" {
  export interface SwalOptions {
    title?: string;
    text?: string;
    html?: string;
    icon?: "success" | "error" | "warning" | "info" | "question";
    toast?: boolean;
    position?: "top" | "top-end" | "center" | "bottom-end";
    timer?: number;
    timerProgressBar?: boolean;
    showConfirmButton?: boolean;
    showCancelButton?: boolean;
    confirmButtonText?: string;
    cancelButtonText?: string;
    confirmButtonColor?: string;
    focusConfirm?: boolean;
  }
  export interface SwalResult {
    isConfirmed: boolean;
    isDismissed: boolean;
  }
  export interface Swal {
    fire(options: SwalOptions): Promise<SwalResult>;
    mixin(options: SwalOptions): Swal;
  }
  const Swal: Swal;
  export default Swal;
}
