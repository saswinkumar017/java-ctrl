import { api, copyToClipboard } from "../lib/api";
import { clear, h } from "../lib/dom";
import { store, refreshJdks } from "../lib/store";
import { friendlyError, notify } from "../lib/toast";
import {
  badge,
  button,
  createCard,
  emptyState,
  keyValues,
  refreshButton,
  spinner,
  statTile,
} from "../lib/ui";
import { errorNotice, pageHead } from "./common";
import type { Cleanup, PageDefinition } from "./page";
import type { JdkInfo } from "../types";

export function jdksPage(): PageDefinition {
  return {
    path: "/jdks",
    title: "JDK Manager",
    description: "Discover installed Java Development Kits and their tools",
    group: "Manage",
    icon: "coffee",
    mount(root): Cleanup {
      root.appendChild(
        pageHead(
          "JDK Manager",
          "Discovers JDKs from JAVA_HOME, PATH, common installation directories and the running processes.",
          [
            refreshButton(() => void load(false)),
            button({ label: "Deep scan", iconName: "search", variant: "primary", onClick: () => void load(true) }),
          ],
        ),
      );

      const stats = h("div", { className: "stat-grid" });
      root.appendChild(stats);
      const list = h("div", { className: "jdk-grid" });
      root.appendChild(list);
      const noteArea = h("div");
      root.insertBefore(noteArea, list);

      let disposed = false;
      let loading = false;

      async function load(force: boolean): Promise<void> {
        if (loading) return;
        loading = true;
        if (store.get().jdks.length === 0) list.appendChild(spinner("Scanning for JDKs…"));
        try {
          await refreshJdks(force);
        } catch (error) {
          if (!disposed) notify.error("JDK scan failed", friendlyError(error));
        } finally {
          loading = false;
          if (!disposed) render();
        }
      }

      function render(): void {
        if (disposed) return;
        const state = store.get();
        const jdks = state.jdks;
        clear(stats);
        stats.appendChild(statTile({ label: "JDKs found", value: String(jdks.length), sub: "on this machine", iconName: "coffee" }));
        stats.appendChild(statTile({
          label: "Default Java",
          value: state.info?.javaHome ? state.info.javaHome.split(/[\\/]/).pop() ?? "—" : "Not detected",
          sub: state.info?.javaHome ?? "set JAVA_HOME to register one",
          iconName: "check",
          tone: state.info?.javaHome ? "ok" : "warn",
        }));
        const withJcmd = jdks.filter((jdk) => jdk.tools.some((tool) => tool.name === "jcmd" && tool.available)).length;
        stats.appendChild(statTile({ label: "Attach-ready", value: String(withJcmd), sub: "JDKs with jcmd", iconName: "terminal", tone: "info" }));

        clear(noteArea);
        const scanNote = jdks.length === 0;
        if (scanNote) {
          noteArea.appendChild(errorNotice(new Error("No JDK installations were detected."), "Nothing found"));
        }

        clear(list);
        if (jdks.length === 0) {
          list.appendChild(
            emptyState({
              iconName: "coffee",
              title: "No JDKs detected",
              description: "Install a JDK 8+ and make sure JAVA_HOME or your PATH points at it, then run a deep scan.",
              actions: [button({ label: "Deep scan", iconName: "search", variant: "primary", onClick: () => void load(true) })],
            }),
          );
          return;
        }
        for (const jdk of jdks) list.appendChild(jdkCard(jdk, state.settings.defaultJdk));
      }

      const unsubscribe = store.subscribe((_, changed) => {
        if (changed.has("jdks") || changed.has("settings")) render();
      });
      if (store.get().jdks.length === 0) void load(false);
      else render();

      return () => {
        disposed = true;
        unsubscribe();
      };
    },
  };
}

function jdkCard(jdk: JdkInfo, defaultJdk: string | null): HTMLElement {
  const isDefault = defaultJdk === jdk.javaHome || (!defaultJdk && jdk.isDefault);
  const card = createCard({
    className: "jdk-card",
    title: jdk.version ? `Java ${jdk.version}` : "Unknown Java version",
    subtitle: jdk.vendor ?? "Unknown vendor",
    iconName: "coffee",
    actions: [isDefault ? badge("DEFAULT", "ok") : badge(jdk.source, "neutral")],
  });
  card.body.appendChild(
    keyValues([
      ["JAVA_HOME", h("span", { className: "mono" }, jdk.javaHome)],
      ["Version major", jdk.versionMajor ? String(jdk.versionMajor) : "—"],
      ["VM", jdk.vmName ?? "—"],
      ["Architecture", jdk.architecture ?? jdk.osArch ?? "—"],
      ["Discovered via", jdk.source],
    ]),
  );
  card.body.appendChild(
    h(
      "div",
      { className: "jdk-tools" },
      ...jdk.tools.map((tool) =>
        h(
          "span",
          {
            className: `chip mono ${tool.available ? "jdk-tool-ok" : "jdk-tool-missing"}`,
            title: tool.path ?? "not available",
          },
          tool.name,
        ),
      ),
    ),
  );
  card.body.appendChild(
    h(
      "div",
      { className: "flex-row", style: { marginTop: "12px" } },
      button({
        label: "Copy path",
        iconName: "copy",
        size: "sm",
        variant: "subtle",
        onClick: () => {
          void copyToClipboard(jdk.javaHome).then(() => notify.ok("JAVA_HOME copied"));
        },
      }),
      button({
        label: "Open folder",
        iconName: "folder",
        size: "sm",
        variant: "subtle",
        onClick: () => {
          void api.openPath(jdk.javaHome).catch((error) => notify.error("Could not open folder", friendlyError(error)));
        },
      }),
      isDefault
        ? null
        : button({
            label: "Set default",
            iconName: "check",
            size: "sm",
            variant: "secondary",
            onClick: () => void setDefault(jdk.javaHome),
          }),
    ),
  );
  return card.root;
}

async function setDefault(javaHome: string): Promise<void> {
  try {
    const current = store.get().settings;
    await api.saveSettings({ ...current, defaultJdk: javaHome });
    store.set({ settings: { ...current, defaultJdk: javaHome } });
    notify.ok("Default JDK updated", javaHome);
  } catch (error) {
    notify.error("Could not save default JDK", friendlyError(error));
  }
}

