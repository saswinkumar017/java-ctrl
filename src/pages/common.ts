import { h, type Child } from "../lib/dom";
import { icon } from "../lib/icons";
import { button, emptyState } from "../lib/ui";
import { friendlyError } from "../lib/toast";
import type { PageContext } from "./page";

export function pageHead(title: string, description: string, actions: Child[] = []): HTMLElement {
  return h(
    "div",
    { className: "page-head" },
    h("div", {}, h("h1", {}, title), h("p", {}, description)),
    actions.length ? h("div", { className: "page-actions" }, ...actions) : null,
  );
}

export function noProcess(context: PageContext, description?: string): HTMLElement {
  return emptyState({
    iconName: "activity",
    title: "No JVM process selected",
    description:
      description
      ?? "Pick a running Java process from the selector in the top bar. JCC attaches with jcmd only — no agent is injected into the target JVM.",
    actions: [
      button({
        label: "Browse processes",
        iconName: "list",
        variant: "primary",
        onClick: () => context.navigate("/processes"),
      }),
    ],
  });
}

export function errorNotice(error: unknown, title = "Could not load data"): HTMLElement {
  return h(
    "div",
    { className: "notice notice-error" },
    icon("alert", 16),
    h("div", {}, h("strong", {}, `${title}. `), friendlyError(error)),
  );
}

export function infoNotice(message: string): HTMLElement {
  return h("div", { className: "notice notice-info" }, icon("info", 16), h("div", {}, message));
}

export function warnNotice(message: string): HTMLElement {
  return h("div", { className: "notice notice-warn" }, icon("alert", 16), h("div", {}, message));
}
