import { appendChildren, h, type Child } from "./dom";
import { icon, type IconName } from "./icons";
import { formatPercent, percentOf } from "./format";

export type Tone = "neutral" | "ok" | "warn" | "danger" | "info" | "accent";

export interface CardHandle {
  root: HTMLElement;
  header: HTMLElement | null;
  body: HTMLElement;
}

export interface CardOptions {
  title?: string;
  subtitle?: string;
  actions?: Child[];
  iconName?: IconName;
  className?: string;
  body?: Child[];
  flush?: boolean;
}

export function createCard(options: CardOptions): CardHandle {
  const header = options.title
    ? h(
        "header",
        { className: "card-header" },
        h(
          "div",
          { className: "card-heading" },
          options.iconName ? h("span", { className: "card-heading-icon" }, icon(options.iconName, 16)) : null,
          h(
            "div",
            {},
            h("h2", { className: "card-title" }, options.title),
            options.subtitle ? h("p", { className: "card-subtitle" }, options.subtitle) : null,
          ),
        ),
        options.actions?.length ? h("div", { className: "card-actions" }, ...options.actions) : null,
      )
    : null;
  const body = h("div", { className: ["card-body", options.flush ? "flush" : ""].filter(Boolean) }, ...(options.body ?? []));
  const root = h(
    "section",
    { className: ["card", options.className ?? ""].filter(Boolean) },
    header,
    body,
  );
  return { root, header, body };
}

export function card(options: CardOptions): HTMLElement {
  return createCard(options).root;
}

export function badge(label: string, tone: Tone = "neutral"): HTMLElement {
  return h("span", { className: `badge badge-${tone}` }, label);
}

export interface ButtonOptions {
  label?: string;
  iconName?: IconName;
  onClick?: (event: MouseEvent) => void;
  variant?: "primary" | "secondary" | "ghost" | "danger" | "subtle";
  size?: "sm" | "md";
  disabled?: boolean;
  title?: string;
  type?: "button" | "submit";
  className?: string;
}

export function button(options: ButtonOptions): HTMLButtonElement {
  const hasLabel = !!options.label;
  return h(
    "button",
    {
      type: options.type ?? "button",
      className: [
        "btn",
        `btn-${options.variant ?? "secondary"}`,
        `btn-${options.size ?? "md"}`,
        !hasLabel ? "btn-icon" : "",
        options.className ?? "",
      ].filter(Boolean),
      onClick: options.onClick,
      disabled: options.disabled ?? false,
      title: options.title ?? (hasLabel ? undefined : options.iconName),
    },
    options.iconName ? icon(options.iconName, options.size === "sm" ? 14 : 16) : null,
    hasLabel ? h("span", {}, options.label) : null,
  );
}

export function iconButton(
  iconName: IconName,
  title: string,
  onClick: (event: MouseEvent) => void,
  variant: ButtonOptions["variant"] = "ghost",
): HTMLButtonElement {
  return button({ iconName, title, onClick, variant, size: "sm" });
}

export function refreshButton(onClick: () => void, busy = false): HTMLButtonElement {
  return button({
    iconName: "refresh",
    label: "Refresh",
    variant: "subtle",
    size: "sm",
    onClick,
    disabled: busy,
  });
}

export interface StatTileOptions {
  label: string;
  value: string;
  sub?: string;
  tone?: Tone;
  iconName?: IconName;
  onClick?: () => void;
}

export function statTile(options: StatTileOptions): HTMLElement {
  const content = h(
    "div",
    { className: ["stat-tile", options.onClick ? "clickable" : "", options.tone ? `tone-${options.tone}` : ""].filter(Boolean), onClick: options.onClick },
    h(
      "div",
      { className: "stat-top" },
      h("span", { className: "stat-label" }, options.label),
      options.iconName ? h("span", { className: "stat-icon" }, icon(options.iconName, 16)) : null,
    ),
    h("div", { className: "stat-value" }, options.value),
    options.sub ? h("div", { className: "stat-sub" }, options.sub) : null,
  );
  return content;
}

export interface TableColumn {
  label: string;
  align?: "left" | "right" | "center";
  className?: string;
  width?: string;
}

export interface TableResult {
  wrapper: HTMLElement;
  table: HTMLTableElement;
  body: HTMLTableSectionElement;
}

export type TableRow = Child[] | HTMLTableRowElement;

export function table(columns: TableColumn[], rows: TableRow[], options: { dense?: boolean; empty?: string } = {}): TableResult {
  const body = h("tbody");
  if (rows.length === 0) {
    body.appendChild(
      h("tr", {}, h("td", { colSpan: Math.max(1, columns.length), className: "table-empty" }, options.empty ?? "Nothing to show yet")),
    );
  } else {
    for (const row of rows) {
      if (row instanceof HTMLTableRowElement) {
        body.appendChild(row);
      } else {
        body.appendChild(h("tr", {}, ...row.map((cell, index) => h("td", {
          className: [columns[index]?.className ?? "", columns[index]?.align === "right" ? "num" : ""].filter(Boolean),
        }, cell))));
      }
    }
  }
  const head = h(
    "thead",
    {},
    h("tr", {}, ...columns.map((column) => h("th", {
      className: [column.align === "right" ? "num" : "", column.className ?? ""].filter(Boolean),
      style: column.width ? { width: column.width } : undefined,
    }, column.label))),
  );
  const element = h("table", { className: options.dense ? "data-table dense" : "data-table" }, head, body);
  return { wrapper: h("div", { className: "table-wrap" }, element), table: element, body };
}

export function progressBar(used: number, max: number, tone: Tone = "accent"): HTMLElement {
  const percentage = percentOf(used, max);
  return h(
    "div",
    { className: "progress-track", title: `${Math.round(percentage)}%` },
    h("div", { className: `progress-fill tone-${tone}`, style: { width: `${percentage}%` } }),
  );
}

export function miniUsage(used: number, max: number, label?: string): HTMLElement {
  return h(
    "div",
    { className: "mini-usage" },
    h(
      "div",
      { className: "mini-usage-track" },
      h("div", {
        className: "mini-usage-fill",
        style: {
          width: `${percentOf(used, max)}%`,
          background: percentageColor(percentOf(used, max)),
        },
      }),
    ),
    label ? h("span", { className: "mini-usage-label" }, label) : null,
  );
}

function percentageColor(percentage: number): string {
  if (percentage >= 85) return "var(--danger)";
  if (percentage >= 65) return "var(--amber)";
  return "var(--accent)";
}

export function field(label: string, control: HTMLElement, hint?: string): HTMLElement {
  return h(
    "label",
    { className: "field" },
    h("span", { className: "field-label" }, label),
    control,
    hint ? h("span", { className: "field-hint" }, hint) : null,
  );
}

export function textInput(placeholder: string, value = "", type = "text"): HTMLInputElement {
  return h("input", { className: "input", type, placeholder, value, spellcheck: false });
}

export interface OptionItem {
  value: string;
  label: string;
}

export function selectInput(options: OptionItem[], value: string, onChange?: (value: string) => void): HTMLSelectElement {
  const select = h(
    "select",
    { className: "input select" },
    ...options.map((option) => h("option", { value: option.value, selected: option.value === value }, option.label)),
  );
  if (onChange) select.addEventListener("change", () => onChange(select.value));
  return select;
}

export interface Checkbox {
  root: HTMLElement;
  readonly checked: boolean;
}

export function checkbox(label: string): Checkbox {
  const input = h("input", { type: "checkbox" });
  const root = h("label", { className: "checkbox-label" }, input, h("span", {}, label));
  return {
    root,
    get checked(): boolean {
      return input.checked;
    },
  };
}

export function searchInput(
  placeholder: string,
  onInput: (value: string) => void,
): { wrapper: HTMLElement; input: HTMLInputElement } {
  const input = h("input", { className: "input input-search", type: "search", placeholder, spellcheck: false });
  input.addEventListener("input", () => onInput(input.value.trim().toLowerCase()));
  return { wrapper: h("div", { className: "search-field" }, icon("search", 15), input), input };
}

export function emptyState(options: {
  iconName: IconName;
  title: string;
  description?: string;
  actions?: Child[];
}): HTMLElement {
  return h(
    "div",
    { className: "empty-state" },
    h("div", { className: "empty-glyph" }, icon(options.iconName, 28)),
    h("h3", {}, options.title),
    options.description ? h("p", {}, options.description) : null,
    options.actions?.length ? h("div", { className: "empty-actions" }, ...options.actions) : null,
  );
}

export function spinner(message?: string): HTMLElement {
  return h(
    "div",
    { className: "spinner-row" },
    h("span", { className: "spinner" }),
    message ? h("span", {}, message) : null,
  );
}

export function toolbar(...children: Child[]): HTMLElement {
  return h("div", { className: "toolbar" }, ...children);
}

export function spacer(): HTMLElement {
  return h("div", { className: "spacer" });
}

export function keyValues(entries: Array<[string, Child]>): HTMLElement {
  return h(
    "dl",
    { className: "kv-grid" },
    ...entries.flatMap(([key, value]) => [
      h("dt", {}, key),
      h("dd", {}, value),
    ]),
  );
}

export function code(text: string, className = ""): HTMLElement {
  return h("pre", { className: ["code-block", className].filter(Boolean) }, text);
}

export function scriptChips(items: string[]): HTMLElement {
  if (items.length === 0) return h("span", { className: "muted" }, "none");
  return h("div", { className: "chip-row" }, ...items.map((item) => h("span", { className: "chip mono", title: item }, item)));
}

export function modal(options: {
  title: string;
  subtitle?: string;
  body: Child[];
  footer?: Child[];
  width?: number;
  onClose?: () => void;
}): { root: HTMLElement; close: () => void } {
  const dialog = h(
    "div",
    { className: "modal-card", style: { width: `${options.width ?? 480}px` }, role: "dialog", "aria-modal": "true" },
    h(
      "header",
      { className: "modal-header" },
      h("div", {}, h("h2", {}, options.title), options.subtitle ? h("p", {}, options.subtitle) : null),
    ),
    h("div", { className: "modal-body" }, ...options.body),
    options.footer?.length ? h("footer", { className: "modal-footer" }, ...options.footer) : null,
  );
  const overlay = h("div", { className: "modal-overlay" }, dialog);
  const close = (): void => {
    overlay.classList.remove("visible");
    window.setTimeout(() => overlay.remove(), 180);
    options.onClose?.();
  };
  overlay.addEventListener("click", (event) => {
    if (event.target === overlay) close();
  });
  document.body.appendChild(overlay);
  requestAnimationFrame(() => overlay.classList.add("visible"));
  return { root: overlay, close };
}

export function confirmAction(options: {
  title: string;
  message: string;
  confirmLabel?: string;
  danger?: boolean;
  extra?: Child[];
}): Promise<boolean> {
  return new Promise((resolve) => {
    let settled = false;
    const finish = (value: boolean): void => {
      if (settled) return;
      settled = true;
      window.removeEventListener("keydown", onKey);
      handle.close();
      resolve(value);
    };
    const onKey = (event: KeyboardEvent): void => {
      if (event.key === "Escape") finish(false);
    };
    window.addEventListener("keydown", onKey);
    const handle = modal({
      title: options.title,
      body: [
        h("p", { className: "modal-message" }, options.message),
        ...(options.extra ?? []),
      ],
      footer: [
        button({ label: "Cancel", variant: "ghost", onClick: () => finish(false) }),
        button({
          label: options.confirmLabel ?? "Confirm",
          variant: options.danger ? "danger" : "primary",
          onClick: () => finish(true),
        }),
      ],
      onClose: () => finish(false),
    });
  });
}

export function panelGrid(...children: Child[]): HTMLElement {
  return h("div", { className: "panel-grid" }, ...children);
}

export function flexRow(...children: Child[]): HTMLElement {
  return h("div", { className: "flex-row" }, ...children);
}

export function append(parent: Element, ...children: Child[]): void {
  appendChildren(parent, children);
}

export function liveDot(state: "live" | "idle" | "error" = "live"): HTMLElement {
  return h("span", { className: `live-dot live-${state}`, title: state });
}

export function percentText(used: number, max: number): string {
  if (!max) return "—";
  return formatPercent((used / max) * 100, 0);
}
