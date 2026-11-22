import { api } from "../lib/api";
import { clear, h } from "../lib/dom";
import { formatBytes, formatDuration, formatTime } from "../lib/format";
import { refreshJdks, refreshProcesses, requirePid, selectProcess, store } from "../lib/store";
import { friendlyError, notify } from "../lib/toast";
import {
  badge,
  button,
  checkbox,
  confirmAction,
  createCard,
  keyValues,
  refreshButton,
  searchInput,
  spinner,
  statTile,
  table,
} from "../lib/ui";
import { errorNotice, pageHead } from "./common";
import type { Cleanup, PageContext, PageDefinition } from "./page";
import type { JfrRecording, JvmProcess } from "../types";

export function processesPage(): PageDefinition {
  return {
    path: "/processes",
    title: "JVM Processes",
    description: "Discover, inspect and manage running Java processes",
    group: "Manage",
    icon: "list",
    mount(root, context): Cleanup {
      root.appendChild(
        pageHead(
          "JVM Processes",
          "Enumerated with the OS process table and jcmd -l. Termination verifies that the PID is really a Java process.",
          [refreshButton(() => void refreshProcesses(false)), button({ label: "Rescan JDKs", iconName: "search", variant: "subtle", onClick: () => void rescan() })],
        ),
      );

      const stats = h("div", { className: "stat-grid" });
      root.appendChild(stats);
      const autoLabel = h("label", { className: "switch" });
      const toggle = h("input", { type: "checkbox", checked: true });
      autoLabel.appendChild(toggle);
      autoLabel.appendChild(h("span", { className: "switch-track" }));
      autoLabel.appendChild(h("span", {}, "Auto-refresh"));

      const search = searchInput("Filter by PID, name, main class, args…", (value) => {
        filter = value;
        renderTable();
      });
      root.appendChild(
        h(
          "div",
          { className: "toolbar" },
          search.wrapper,
          autoLabel,
          h("span", { className: "muted", style: { fontSize: "11px" } }, "Updates every 6 seconds"),
        ),
      );

      const tableHost = h("div");
      root.appendChild(tableHost);
      const detailCard = createCard({
        title: "Process detail",
        subtitle: "Runtime information read with jcmd VM.command_line and VM.version",
        iconName: "terminal",
      });
      root.appendChild(detailCard.root);
      const jfrCard = createCard({ title: "JFR recordings", subtitle: "Java Flight Recorder sessions in the target JVM", iconName: "clock" });
      root.appendChild(jfrCard.root);

      let filter = "";
      let disposed = false;
      let auto = true;

      toggle.addEventListener("change", () => {
        auto = toggle.checked;
      });

      function renderTable(): void {
        if (disposed) return;
        const processes = store.get().processes;
        const selected = requirePid();
        clear(stats);
        stats.appendChild(statTile({ label: "Java processes", value: String(processes.length), sub: "currently running", iconName: "server" }));
        stats.appendChild(statTile({
          label: "Attachable",
          value: String(processes.filter((process) => process.attachable).length),
          sub: "jcmd-compatible",
          iconName: "terminal",
          tone: "ok",
        }));
        stats.appendChild(statTile({
          label: "Total RSS",
          value: formatBytes(processes.reduce((sum, process) => sum + process.rssBytes, 0)),
          sub: "resident memory",
          iconName: "memory",
          tone: "info",
        }));

        const visible = processes.filter((process) => {
          if (!filter) return true;
          const haystack = [
            String(process.pid), process.name, process.displayName, process.mainClass ?? "",
            process.mainJar ?? "", process.javaVersion ?? "", process.commandLine,
          ].join(" ").toLowerCase();
          return haystack.includes(filter);
        });
        const view = table(
          [
            { label: "PID", width: "76px" },
            { label: "Process" },
            { label: "Main class / JAR" },
            { label: "Java", width: "96px" },
            { label: "CPU", align: "right", width: "74px" },
            { label: "RSS", align: "right", width: "96px" },
            { label: "Uptime", align: "right", width: "90px" },
            { label: "Spring", width: "80px" },
            { label: "Actions", align: "right", width: "150px" },
          ],
          visible.map((process) => [
            h("span", { className: "mono" }, String(process.pid)),
            h(
              "div",
              { className: "cell-stack" },
              h("span", {}, process.displayName),
              h("span", { className: "cell-sub mono" }, process.executable ?? process.name),
            ),
            h(
              "div",
              { className: "cell-stack" },
              h("span", { className: "mono ellipsis", title: process.mainClass ?? process.mainJar ?? "" }, process.mainClass ?? process.mainJar ?? "—"),
              h("span", { className: "cell-sub" }, process.cwd ?? ""),
            ),
            process.javaVersion ? badge(`Java ${process.javaVersion}`, "accent") : h("span", { className: "muted" }, "—"),
            `${process.cpuPercent.toFixed(1)}%`,
            formatBytes(process.rssBytes),
            formatDuration(process.uptimeSecs),
            process.springPorts.length ? badge(`:${process.springPorts[0]}`, "info") : h("span", { className: "muted" }, "—"),
            h(
              "div",
              { className: "flex-row", style: { justifyContent: "flex-end", flexWrap: "nowrap" } },
              button({
                label: selected === process.pid ? "Selected" : "Select",
                iconName: selected === process.pid ? "check" : "arrowRight",
                size: "sm",
                variant: selected === process.pid ? "subtle" : "primary",
                onClick: () => {
                  selectProcess(process.pid);
                  notify.ok(`Selected PID ${process.pid}`, process.displayName);
                },
              }),
              button({
                iconName: "zap",
                size: "sm",
                variant: "ghost",
                title: "Shut down JVM",
                onClick: () => void terminate(process, context),
              }),
            ),
          ]),
          { empty: "No Java processes match the filter" },
        );
        clear(tableHost);
        tableHost.appendChild(view.wrapper);
      }

      async function renderDetail(): Promise<void> {
        clear(detailCard.body);
        clear(jfrCard.body);
        const pid = requirePid();
        const process = store.get().processes.find((item) => item.pid === pid);
        if (!pid || !process) {
          detailCard.body.appendChild(h("p", { className: "muted" }, "Select a process to see its runtime details."));
          jfrCard.body.appendChild(h("p", { className: "muted" }, "No process selected."));
          return;
        }
        detailCard.body.appendChild(spinner("Reading VM details…"));
        jfrCard.body.appendChild(spinner("Checking JFR…"));
        try {
          const [details, recordings] = await Promise.all([api.getRuntimeDetails(pid), api.checkJfr(pid)]);
          if (disposed) return;
          clear(detailCard.body);
          detailCard.body.appendChild(
            keyValues([
              ["Java command", details.javaCommand ?? process.mainClass ?? "—"],
              ["Launcher", details.launcherType ?? "—"],
              ["Class path", h("span", { className: "mono", title: details.classPath ?? "" }, details.classPath ?? "—")],
              ["JVM arguments", details.jvmArgs.join(" ") || "—"],
              ["Started", formatTime(process.startTime)],
              ["Command line", h("span", { className: "mono", title: process.commandLine }, process.commandLine)],
              ["Properties", `${Object.keys(details.properties).length} values`],
            ]),
          );
          renderJfr(recordings, pid);
        } catch (error) {
          if (disposed) return;
          clear(detailCard.body);
          detailCard.body.appendChild(errorNotice(error, "Runtime details unavailable"));
          clear(jfrCard.body);
          jfrCard.body.appendChild(h("p", { className: "muted" }, "JFR status unavailable for this process."));
        }
      }

      function renderJfr(recordings: JfrRecording[], pid: number): void {
        clear(jfrCard.body);
        jfrCard.body.appendChild(
          h(
            "div",
            { className: "toolbar" },
            button({
              label: "Start recording",
              iconName: "play",
              size: "sm",
              variant: "secondary",
              onClick: () => void withInput("Recording name", "jcc-session", (value) => void startJfr(pid, value, context)),
            }),
            button({
              label: "Dump now",
              iconName: "download",
              size: "sm",
              variant: "subtle",
              onClick: () => void withInput("Recording name", "jcc-dump", (value) => void dumpJfr(pid, value, context)),
            }),
          ),
        );
        const view = table(
          [
            { label: "ID", width: "60px" },
            { label: "Name" },
            { label: "State" },
            { label: "Duration" },
            { label: "Destination" },
            { label: "Actions", align: "right" },
          ],
          recordings.map((recording) => [
            String(recording.id),
            recording.name ?? "—",
            badge(recording.state ?? "unknown", recording.state === "running" ? "ok" : "neutral"),
            recording.duration ?? "—",
            h("span", { className: "mono ellipsis", title: recording.destination ?? "" }, recording.destination ?? "—"),
            button({
              label: "Stop",
              iconName: "square",
              size: "sm",
              variant: "danger",
              onClick: () =>
                void api
                  .stopJfr(pid, recording.name ?? String(recording.id))
                  .then(() => {
                    notify.ok("Recording stopped");
                    void renderJfrFromApi(pid);
                  })
                  .catch((error) => notify.error("Could not stop recording", friendlyError(error))),
            }),
          ]),
          { dense: true, empty: "No active JFR recordings. Start one to profile allocations or execution." },
        );
        jfrCard.body.appendChild(view.wrapper);
      }

      async function renderJfrFromApi(pid: number): Promise<void> {
        try {
          renderJfr(await api.checkJfr(pid), pid);
        } catch (error) {
          notify.error("Could not refresh JFR", friendlyError(error));
        }
      }

      const unsubscribe = store.subscribe((_, changed) => {
        if (changed.has("processes") || changed.has("selectedPid")) {
          renderTable();
          void renderDetail();
        }
      });
      renderTable();
      void renderDetail();

      async function rescan(): Promise<void> {
        try {
          await refreshJdks(true);
          notify.ok("JDKs rescanned");
        } catch (error) {
          notify.error("Rescan failed", friendlyError(error));
        }
      }
      const timer = window.setInterval(() => {
        if (auto && !document.hidden) void refreshProcesses(true);
      }, 6000);

      return () => {
        disposed = true;
        window.clearInterval(timer);
        unsubscribe();
      };
    },
  };
}

function withInput(title: string, defaultValue: string, callback: (value: string) => void): void {
  const input = h("input", { className: "input", type: "text", value: defaultValue });
  let closed = false;
  const finish = (value: string): void => {
    if (closed) return;
    closed = true;
    handle.close();
    if (value.trim()) callback(value.trim());
  };
  const handle = ((): { close: () => void } => {
    const overlay = h(
      "div",
      { className: "modal-overlay" },
      h(
        "div",
        {
          className: "modal-card",
          style: { width: "400px" },
          onKeyDown: (event: KeyboardEvent) => {
            if (event.key === "Enter") finish(input.value);
          },
        },
        h("header", { className: "modal-header" }, h("h2", {}, title)),
        h("div", { className: "modal-body" }, h("label", { className: "field" }, h("span", { className: "field-label" }, "Name"), input)),
        h(
          "footer",
          { className: "modal-footer" },
          button({ label: "Cancel", variant: "ghost", onClick: () => finish("") }),
          button({ label: "Continue", variant: "primary", onClick: () => finish(input.value) }),
        ),
      ),
    );
    document.body.appendChild(overlay);
    requestAnimationFrame(() => {
      overlay.classList.add("visible");
      input.focus();
      input.select();
    });
    return { close: () => overlay.remove() };
  })();
}

async function startJfr(pid: number, name: string, context: PageContext): Promise<void> {
  try {
    await api.startJfr(pid, name);
    notify.ok("JFR recording started", name);
    context.navigate("/monitor");
  } catch (error) {
    notify.error("Could not start JFR", friendlyError(error));
  }
}

async function dumpJfr(pid: number, name: string, context: PageContext): Promise<void> {
  try {
    const dump = await api.dumpJfr(pid, name);
    notify.ok("JFR dump saved", dump.path);
    context.navigate("/heap");
  } catch (error) {
    notify.error("Could not dump JFR", friendlyError(error));
  }
}

async function terminate(process: JvmProcess, context: PageContext): Promise<void> {
  const force = checkbox("Force terminate immediately (SIGKILL / taskkill /F) — shutdown hooks will NOT run");
  const confirmed = await confirmAction({
    title: `Shut down PID ${process.pid}?`,
    message: `This will ask ${process.displayName} to shut down. The JVM is verified before any signal is sent. Running requests may be interrupted.`,
    confirmLabel: "Shut down",
    danger: true,
    extra: [force.root],
  });
  if (!confirmed) return;
  try {
    const result = await api.terminateProcess(process.pid, force.checked);
    notify.ok(`Terminate request sent for PID ${result.pid}`, result.message);
    if (requirePid() === process.pid) selectProcess(null);
    await refreshProcesses(false);
    if (requirePid() === null) context.navigate("/processes");
  } catch (error) {
    notify.error("Termination failed", friendlyError(error));
  }
}

