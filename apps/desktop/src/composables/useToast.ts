import { ref, type Ref } from "vue";

type ToastType = "info" | "warning" | "error" | "critical" | "success";

interface ToastAction {
  label: string;
  onClick: () => void;
}

interface ToastOptions {
  type?: ToastType;
  duration?: number;
  action?: ToastAction;
}

interface ToastState {
  message: Ref<string>;
  visible: Ref<boolean>;
  timer: number;
  type: Ref<ToastType>;
  action: Ref<ToastAction | undefined>;
}

declare global {
  var __DBX_TOAST_STATE__: ToastState | undefined;
}

const toastState =
  globalThis.__DBX_TOAST_STATE__ ??
  (globalThis.__DBX_TOAST_STATE__ = {
    message: ref(""),
    visible: ref(false),
    timer: 0,
    type: ref<"info" | "warning" | "error" | "critical" | "success">("info"),
    action: ref(undefined),
  });

export function useToast() {
  function toast(msg: string, optionsOrDuration?: ToastOptions | number) {
    const options = typeof optionsOrDuration === "number" ? { duration: optionsOrDuration } : (optionsOrDuration ?? {});

    toastState.message.value = msg;
    toastState.type.value = options.type ?? "info";
    toastState.action.value = options.action;
    toastState.visible.value = true;
    clearTimeout(toastState.timer);
    toastState.timer = window.setTimeout(() => {
      toastState.visible.value = false;
      toastState.action.value = undefined;
    }, options.duration ?? 2000);
  }

  function success(msg: string, options?: Omit<ToastOptions, "type">) {
    toast(msg, { ...options, type: "success" });
  }

  function error(msg: string, options?: Omit<ToastOptions, "type">) {
    toast(msg, { ...options, type: "error", duration: options?.duration ?? 4000 });
  }

  function warning(msg: string, options?: Omit<ToastOptions, "type">) {
    toast(msg, { ...options, type: "warning" });
  }

  function info(msg: string, options?: Omit<ToastOptions, "type">) {
    toast(msg, { ...options, type: "info" });
  }

  return {
    message: toastState.message,
    visible: toastState.visible,
    type: toastState.type,
    action: toastState.action,
    toast,
    success,
    error,
    warning,
    info,
  };
}
