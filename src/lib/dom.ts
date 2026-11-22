export type Child = Node | string | number | null | undefined | boolean;
type ClassValue = string | false | null | undefined | ClassValue[];

export interface Props {
  className?: ClassValue;
  style?: Partial<CSSStyleDeclaration>;
  dataset?: Record<string, string>;
  html?: string;
  [key: string]: unknown;
}

function applyClass(el: Element, value: ClassValue): void {
  if (Array.isArray(value)) {
    value.forEach((part) => applyClass(el, part));
    return;
  }
  if (value && typeof value === "string") {
    el.classList.add(...value.split(/\s+/).filter(Boolean));
  }
}

export function appendChildren(parent: Node, children: Child[]): void {
  for (const child of children) {
    if (child == null || child === false || child === true) continue;
    if (Array.isArray(child)) {
      appendChildren(parent, child as Child[]);
    } else if (child instanceof Node) {
      parent.appendChild(child);
    } else {
      parent.appendChild(document.createTextNode(String(child)));
    }
  }
}

export function h<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  props?: Props | null,
  ...children: Child[]
): HTMLElementTagNameMap[K] {
  const el = document.createElement(tag);
  if (props) {
    for (const [key, raw] of Object.entries(props)) {
      if (raw == null || raw === false || raw === true && key.startsWith("data-")) continue;
      if (key === "className" || key === "class") {
        applyClass(el, raw as ClassValue);
      } else if (key === "style" && typeof raw === "object") {
        Object.assign(el.style, raw as Partial<CSSStyleDeclaration>);
      } else if (key === "dataset" && typeof raw === "object") {
        for (const [dataKey, dataValue] of Object.entries(raw as Record<string, string>)) {
          el.setAttribute(`data-${dataKey}`, dataValue);
        }
      } else if (key === "html") {
        el.innerHTML = String(raw);
      } else if (key.startsWith("on") && typeof raw === "function") {
        el.addEventListener(key.slice(2).toLowerCase(), raw as EventListener);
      } else if (key === "for") {
        el.setAttribute("for", String(raw));
      } else if (key in el && key !== "list") {
        (el as unknown as Record<string, unknown>)[key] = raw;
      } else {
        el.setAttribute(key, String(raw));
      }
    }
  }
  appendChildren(el, children);
  return el;
}

export function text(value: string | number): Text {
  return document.createTextNode(String(value));
}

export function fragment(...children: Child[]): DocumentFragment {
  const result = document.createDocumentFragment();
  appendChildren(result, children);
  return result;
}

export function clear(el: Element): void {
  while (el.firstChild) el.removeChild(el.firstChild);
}

export function qs<T extends Element = HTMLElement>(selector: string, root: ParentNode = document): T | null {
  return root.querySelector<T>(selector);
}

export function qsa<T extends Element = HTMLElement>(selector: string, root: ParentNode = document): T[] {
  return Array.from(root.querySelectorAll<T>(selector));
}

export function addClass(el: Element, className: string): void {
  el.classList.add(className);
}

export function removeClass(el: Element, className: string): void {
  el.classList.remove(className);
}

export function toggleClass(el: Element, className: string, force?: boolean): void {
  el.classList.toggle(className, force);
}

export interface Poller {
  stop(): void;
  refresh(): void;
}

export function poller(task: () => void | Promise<void>, intervalMs: number, immediate = true): Poller {
  let timer: number | undefined;
  let stopped = false;
  let running = false;
  const run = async (): Promise<void> => {
    if (stopped || running) return;
    running = true;
    try {
      await task();
    } catch (error) {
      console.error("Poll task failed", error);
    } finally {
      running = false;
    }
  };
  if (immediate) void run();
  timer = window.setInterval(() => void run(), Math.max(250, intervalMs));
  return {
    stop() {
      stopped = true;
      if (timer != null) window.clearInterval(timer);
    },
    refresh() {
      void run();
    },
  };
}
