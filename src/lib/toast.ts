import { h } from "./dom";
import { icon, type IconName } from "./icons";

export type ToastTone = "ok" | "error" | "info" | "warn";

let root: HTMLElement | null = null;

function container(): HTMLElement {
  if (root && root.isConnected) return root;
  root = h("div", { className: "toast-stack", role: "status", "aria-live": "polite" });
  document.body.appendChild(root);
  return root;
}

export interface ToastOptions {
  tone?: ToastTone;
  title: string;
  message?: string;
  timeout?: number;
}

const TONE_ICON: Record<ToastTone, IconName> = {
  ok: "check",
  error: "alert",
  info: "info",
  warn: "alert",
};

export function toast({ tone = "info", title, message, timeout = 4800 }: ToastOptions): void {
  const item = h(
    "div",
    { className: `toast toast-${tone}` },
    h("span", { className: `toast-glyph tone-${tone}` }, icon(TONE_ICON[tone], 16)),
    h(
      "div",
      { className: "toast-body" },
      h("div", { className: "toast-title" }, title),
      message ? h("div", { className: "toast-message" }, message) : null,
    ),
    h(
      "button",
      {
        className: "toast-close",
        type: "button",
        title: "Dismiss",
        onClick: () => item.remove(),
      },
      icon("x", 14),
    ),
    h("span", { className: "toast-timer" }),
  );
  container().appendChild(item);
  requestAnimationFrame(() => item.classList.add("visible"));
  const lifetime = timeout <= 0 ? 60_000 : timeout;
  window.setTimeout(() => {
    item.classList.remove("visible");
    window.setTimeout(() => item.remove(), 260);
  }, lifetime);
}

export const notify = {
  ok(title: string, message?: string): void {
    toast({ tone: "ok", title, message });
  },
  error(title: string, message?: string): void {
    toast({ tone: "error", title, message, timeout: 7000 });
  },
  warn(title: string, message?: string): void {
    toast({ tone: "warn", title, message, timeout: 6000 });
  },
  info(title: string, message?: string): void {
    toast({ tone: "info", title, message });
  },
};

export function friendlyError(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  if (error && typeof error === "object") {
    const value = error as Record<string, unknown>;
    if (typeof value.message === "string") return value.message;
  }
  return "Unexpected error";
}

export async function withToast<T>(
  action: Promise<T>,
  success: string | ((value: T) => string),
  failure = "Operation failed",
): Promise<T | undefined> {
  try {
    const value = await action;
    notify.ok(typeof success === "function" ? success(value) : success);
    return value;
  } catch (error) {
    notify.error(failure, friendlyError(error));
    return undefined;
  }
}
