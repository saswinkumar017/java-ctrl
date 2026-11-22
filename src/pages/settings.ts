import { api } from "../lib/api";
import { h } from "../lib/dom";
import { refreshJdks, store } from "../lib/store";
import { friendlyError, notify } from "../lib/toast";
import {
  button,
  createCard,
  field,
  keyValues,
  refreshButton,
  selectInput,
  textInput,
} from "../lib/ui";
import { infoNotice, pageHead, warnNotice } from "./common";
import type { PageDefinition } from "./page";

export function settingsPage(): PageDefinition {
  return {
    path: "/settings",
    title: "Settings",
    description: "Sampling, retention and storage",
    group: "System",
    icon: "settings",
    mount(root): void {
      root.appendChild(
        pageHead(
          "Settings",
          "Sampling frequency, local retention and application information.",
          [refreshButton(() => void load())],
        ),
      );
      const content = h("div", { className: "stack" });
      root.appendChild(content);
      const settings = store.get().settings;

      const behaviour = createCard({ title: "Monitoring", subtitle: "How often JCC samples the selected JVM", iconName: "activity" });
      const interval = selectInput(
        [
          { value: "1000", label: "1 second" },
          { value: "2000", label: "2 seconds" },
          { value: "3000", label: "3 seconds" },
          { value: "5000", label: "5 seconds" },
          { value: "10000", label: "10 seconds" },
        ],
        String(settings.refreshIntervalMs),
      );
      const samples = textInput("30", String(settings.sampleRetentionMinutes), "number");
      const retention = textInput("7", String(settings.retentionDays), "number");
      behaviour.body.appendChild(
        h(
          "div",
          { className: "stack", style: { gap: "14px" } },
          field("Sample interval", interval, "Lower values increase jcmd calls against the target JVM."),
          h("div", { className: "form-grid" },
            field("Sample retention (minutes)", samples),
            field("Dump / event retention (days)", retention),
          ),
        ),
      );
      content.appendChild(behaviour.root);

      const jdkCard = createCard({ title: "Default JDK", subtitle: "Used as the preferred jcmd source", iconName: "coffee" });
      const jdkOptions = [{ value: "", label: "Auto-detect" }, ...store.get().jdks.map((jdk) => ({
        value: jdk.javaHome,
        label: `${jdk.version ? `Java ${jdk.version}` : "Unknown"} — ${jdk.javaHome}`,
      }))];
      const jdkSelect = selectInput(jdkOptions, settings.defaultJdk ?? "");
      jdkCard.body.appendChild(field("Preferred JAVA_HOME", jdkSelect));
      content.appendChild(jdkCard.root);

      const actions = h(
        "div",
        { className: "flex-row" },
        button({
          label: "Save settings",
          iconName: "check",
          variant: "primary",
          onClick: () => void save(),
        }),
        button({ label: "Reset to defaults", iconName: "recycle", variant: "subtle", onClick: () => void reset() }),
        button({ label: "Rescan JDKs", iconName: "search", variant: "subtle", onClick: () => void rescan() }),
      );
      content.appendChild(actions);

      const storageCard = createCard({ title: "Local storage", subtitle: "SQLite database and dump files", iconName: "database" });
      content.appendChild(storageCard.root);
      storageCard.body.appendChild(storageInfo());

      const aboutCard = createCard({ title: "About Java Control Centre", subtitle: "Desktop build information", iconName: "info" });
      content.appendChild(aboutCard.root);
      aboutCard.body.appendChild(
        keyValues([
          ["Version", store.get().info?.version ?? "—"],
          ["Platform", `${store.get().info?.platform ?? "—"} · ${store.get().info?.arch ?? "—"}`],
          ["Default Java", store.get().info?.javaHome ?? "not detected"],
          ["Data directory", store.get().info?.dataDir ?? "—"],
          ["Dumps directory", store.get().info?.dumpsDir ?? "—"],
          ["Frontend", "Vanilla TypeScript + ECharts"],
          ["Backend", "Rust + Tauri 2 + SQLite"],
        ]),
      );
      aboutCard.body.appendChild(
        warnNotice("Actuator passwords are stored in the local SQLite database in plain text. Use a dedicated, low-privilege monitoring account."),
      );

      async function save(): Promise<void> {
        const next = {
          defaultJdk: jdkSelect.value || null,
          refreshIntervalMs: Number(interval.value) || 2000,
          retentionDays: Math.max(1, Number(retention.value) || 7),
          sampleRetentionMinutes: Math.max(1, Number(samples.value) || 30),
        };
        try {
          const saved = await api.saveSettings(next);
          store.set({ settings: saved });
          notify.ok("Settings saved", "Old samples and dumps beyond retention have been pruned.");
          storageCard.body.replaceChildren(storageInfo());
        } catch (error) {
          notify.error("Could not save settings", friendlyError(error));
        }
      }

      async function reset(): Promise<void> {
        const defaults = {
          defaultJdk: null,
          refreshIntervalMs: 2000,
          retentionDays: 7,
          sampleRetentionMinutes: 30,
        };
        interval.value = String(defaults.refreshIntervalMs);
        samples.value = String(defaults.sampleRetentionMinutes);
        retention.value = String(defaults.retentionDays);
        jdkSelect.value = "";
        try {
          const saved = await api.saveSettings(defaults);
          store.set({ settings: saved });
          notify.ok("Defaults restored");
          storageCard.body.replaceChildren(storageInfo());
        } catch (error) {
          notify.error("Could not reset settings", friendlyError(error));
        }
      }

      async function rescan(): Promise<void> {
        try {
          await refreshJdks(true);
          notify.ok("JDKs rescanned");
        } catch (error) {
          notify.error("Rescan failed", friendlyError(error));
        }
      }

      async function load(): Promise<void> {
        try {
          const value = await api.getSettings();
          store.set({ settings: value });
          interval.value = String(value.refreshIntervalMs);
          samples.value = String(value.sampleRetentionMinutes);
          retention.value = String(value.retentionDays);
          jdkSelect.value = value.defaultJdk ?? "";
          storageCard.body.replaceChildren(storageInfo());
        } catch (error) {
          notify.error("Could not load settings", friendlyError(error));
        }
      }
    },
  };
}

function storageInfo(): HTMLElement {
  const info = store.get().info;
  const wrap = h("div", { className: "stack" });
  wrap.appendChild(
    keyValues([
      ["Database", `${info?.dataDir ?? "—"}\\jcc.db`],
      ["Dumps", info?.dumpsDir ?? "—"],
      ["Retention", `${store.get().settings.retentionDays} days · samples ${store.get().settings.sampleRetentionMinutes} min`],
    ]),
  );
  wrap.appendChild(
    h(
      "div",
      { className: "flex-row", style: { marginTop: "12px" } },
      button({
        label: "Open dumps folder",
        iconName: "folder",
        size: "sm",
        variant: "secondary",
        onClick: () => void api.openPath(info?.dumpsDir ?? "").catch((error) => notify.error("Could not open folder", friendlyError(error))),
      }),
      button({
        label: "Prune now",
        iconName: "trash",
        size: "sm",
        variant: "subtle",
        onClick: () => void api.pruneStorage().then(() => notify.ok("Storage pruned")).catch((error) => notify.error("Prune failed", friendlyError(error))),
      }),
    ),
  );
  wrap.appendChild(infoNotice("Pruning removes samples older than the sample retention window and dump records older than the retention period."));
  return wrap;
}

