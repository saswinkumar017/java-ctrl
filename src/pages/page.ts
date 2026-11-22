import type { JvmProcess } from "../types";
import type { IconName } from "../lib/icons";
import type { Child } from "../lib/dom";

export interface PageContext {
  pid: number | null;
  process: JvmProcess | null;
  navigate: (path: string) => void;
  refreshProcesses: () => Promise<void>;
}

export interface PageDefinition {
  path: string;
  title: string;
  description: string;
  group: string;
  icon: IconName;
  mount: (root: HTMLElement, context: PageContext) => void | (() => void);
}

export type PageFactory = () => PageDefinition;

export interface RegisteredRoute extends PageDefinition {
  factory: PageFactory;
}

export type Cleanup = void | (() => void);

export function collect(cleanups: Array<() => void>): () => void {
  return () => {
    for (const cleanup of cleanups) cleanup();
  };
}

export function add(cleanups: Array<() => void>, value: void | (() => void)): void {
  if (typeof value === "function") cleanups.push(value);
}

export type { Child };
