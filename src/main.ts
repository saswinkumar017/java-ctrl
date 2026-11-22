import "./styles.css";
import { clear, h, qs } from "./lib/dom";
import { backendAvailable } from "./lib/api";
import { brand, icon } from "./lib/icons";
import { bootstrap, refreshProcesses, selectProcess, store } from "./lib/store";
import { friendlyError, notify } from "./lib/toast";
import { iconButton, liveDot } from "./lib/ui";
import { gcPage } from "./pages/gc";
import { classesPage } from "./pages/classes";
import { heapPage } from "./pages/heap";
import { jdksPage } from "./pages/jdks";
import { jitPage } from "./pages/jit";
import { memoryPage } from "./pages/memory";
import { monitorPage } from "./pages/monitor";
import { overviewPage } from "./pages/overview";
import { processesPage } from "./pages/processes";
import { settingsPage } from "./pages/settings";
import { springPage } from "./pages/spring";
import { threadsPage } from "./pages/threads";
import type { PageContext, RegisteredRoute } from "./pages/page";

const routeFactories = [
  overviewPage,
  monitorPage,
  memoryPage,
  gcPage,
  threadsPage,
  classesPage,
  jitPage,
  heapPage,
  springPage,
  processesPage,
  jdksPage,
  settingsPage,
];

const routes: RegisteredRoute[] = routeFactories.map((factory) => ({ ...factory(), factory }));

function currentPath(): string {
  const hash = window.location.hash.replace(/^#/, "");
  return routes.some((route) => route.path === hash) ? hash : "/overview";
}

const app = qs<HTMLDivElement>("#app");
if (!app) throw new Error("Missing #app root");
clear(app);

const navHost = h("nav", { className: "nav-scroll" });
const crumbGroup = h("span", {}, "OBSERVE");
const crumbTitle = h("strong", {}, "Overview");
const statusDot = liveDot("live");
const processSelect = h("select", { className: "select", title: "Selected JVM process" });
const processCount = h("span", { className: "badge badge-neutral" }, "0");
const backendStatus = h("span", { className: "flex-row", style: { gap: "6px", fontSize: "11px" } }, statusDot, h("span", {}, "Ready"));
const bannerHost = h("div");
const pageHost = h("main", { className: "content" }, bannerHost);

const sidebar = h(
  "aside",
  { className: "sidebar" },
  h(
    "div",
    { className: "brand" },
    brand(32),
    h("div", { className: "brand-copy" }, h("span", { className: "brand-name" }, "Java Control Centre"), h("span", { className: "brand-sub" }, "Runtime Operations")),
  ),
  navHost,
  h(
    "div",
    { className: "sidebar-footer" },
    h("div", { className: "sidebar-status" }, statusDot, h("span", {}, "jcmd attach mode")),
    h("div", {}, `JCC · v${store.get().info?.version ?? "0.1.0"}`),
  ),
);

const topbar = h(
  "header",
  { className: "topbar" },
  h("div", { className: "crumb" }, crumbGroup, h("span", {}, "/"), crumbTitle),
  h("div", { className: "topbar-spacer" }),
  backendStatus,
  h("div", { className: "topbar-divider" }),
  h(
    "div",
    { className: "process-picker" },
    h("span", { className: "process-picker-label" }, "JVM"),
    processSelect,
    processCount,
  ),
  iconButton("refresh", "Refresh process list", () => void refreshProcesses(false)),
  iconButton("settings", "Settings", () => navigate("/settings")),
);

const shell = h("div", { className: "app-shell" }, sidebar, h("div", { className: "main" }, topbar, pageHost));
app.appendChild(shell);

const navButtons = new Map<string, HTMLButtonElement>();
const groups = ["Observe", "Inspect", "Manage", "System"];
for (const group of groups) {
  const section = h("div", { className: "nav-group" }, h("div", { className: "nav-group-title" }, group));
  for (const route of routes.filter((item) => item.group === group)) {
    const button = h(
      "button",
      {
        className: "nav-item",
        type: "button",
        onClick: () => navigate(route.path),
      },
      icon(route.icon, 17),
      h("span", {}, route.title),
    );
    navButtons.set(route.path, button);
    section.appendChild(button);
  }
  navHost.appendChild(section);
}

processSelect.addEventListener("change", () => {
  const value = processSelect.value;
  selectProcess(value ? Number(value) : null);
});

let cleanup: void | (() => void);
let currentRoutePath = "";

function renderRoute(): void {
  if (typeof cleanup === "function") cleanup();
  const path = currentPath();
  const route = routes.find((item) => item.path === path) ?? routes[0]!;
  currentRoutePath = route.path;
  clear(pageHost);
  pageHost.appendChild(bannerHost);
  renderBanner();

  crumbGroup.textContent = route.group.toUpperCase();
  crumbTitle.textContent = route.title;
  for (const [key, button] of navButtons) button.classList.toggle("active", key === route.path);

  const state = store.get();
  const context: PageContext = {
    pid: state.selectedPid,
    process: state.processes.find((process) => process.pid === state.selectedPid) ?? null,
    navigate,
    refreshProcesses: () => refreshProcesses(false),
  };
  const page = h("div", { className: "page" });
  pageHost.appendChild(page);
  cleanup = route.mount(page, context);
  pageHost.scrollTop = 0;
}

function renderBanner(): void {
  clear(bannerHost);
  if (!backendAvailable()) {
    bannerHost.appendChild(
      h("div", { className: "notice notice-error" },
        icon("alert", 16),
        h("div", {}, h("strong", {}, "Desktop backend unavailable. "), "Run this app with Tauri (`npm run tauri:dev`) or install a release build; a plain browser cannot call the Rust backend."),
      ),
    );
    return;
  }
  const error = store.get().backendError;
  if (error) {
    bannerHost.appendChild(
      h("div", { className: "notice notice-warn" },
        icon("alert", 16),
        h("div", {}, h("strong", {}, "Backend warning: "), friendlyError(error)),
      ),
    );
  }
}

function renderProcessPicker(): void {
  const state = store.get();
  const previous = processSelect.value;
  clear(processSelect);
  processSelect.appendChild(h("option", { value: "" }, "No process selected"));
  for (const process of state.processes) {
    const label = `${process.displayName} · ${process.pid} · ${process.javaVersion ? `Java ${process.javaVersion}` : "Java"}`;
    processSelect.appendChild(h("option", { value: String(process.pid), selected: String(process.pid) === String(state.selectedPid) }, label));
  }
  processSelect.value = state.selectedPid != null ? String(state.selectedPid) : "";
  if (processSelect.value !== previous) processSelect.dispatchEvent(new Event("change"));
  processCount.textContent = String(state.processes.length);
  processCount.className = `badge ${state.processes.length ? "badge-accent" : "badge-neutral"}`;
}

function renderStatus(): void {
  const state = store.get();
  const tone = state.backendError ? "error" : state.processesLoading ? "idle" : "live";
  statusDot.className = `live-dot live-${tone}`;
  const count = state.processes.length;
  const text = state.backendError ? "Backend warning" : state.processesLoading ? "Scanning" : `${count} JVM${count === 1 ? "" : "s"}`;
  const label = backendStatus.querySelector("span:last-child");
  if (label) label.textContent = text;
  const footerLabel = sidebar.querySelector(".sidebar-status span:last-child");
  if (footerLabel) footerLabel.textContent = state.backendError ? "backend warning" : "jcmd attach mode";
}

store.subscribe((_, changed) => {
  if (changed.has("processes") || changed.has("selectedPid")) {
    renderProcessPicker();
    renderStatus();
    if (changed.has("selectedPid") && currentRoutePath !== "/processes") renderRoute();
  }
  if (changed.has("backendError")) renderBanner();
  if (changed.has("info")) {
    const version = sidebar.querySelector(".sidebar-footer div:last-child");
    if (version) version.textContent = `JCC · v${store.get().info?.version ?? "0.1.0"}`;
  }
});

window.addEventListener("hashchange", renderRoute);

export function navigate(path: string): void {
  if (window.location.hash === `#${path}`) renderRoute();
  else window.location.hash = path;
}

renderProcessPicker();
renderStatus();
renderRoute();

void bootstrap().then(() => {
  renderProcessPicker();
  renderStatus();
  renderBanner();
});

window.setInterval(() => {
  if (!document.hidden) void refreshProcesses(true);
}, 6000);

window.addEventListener("unhandledrejection", (event) => {
  console.error("Unhandled promise rejection", event.reason);
  notify.error("Unexpected error", friendlyError(event.reason));
});

