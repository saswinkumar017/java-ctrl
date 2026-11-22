import { api } from "../lib/api";
import { ChartHost, palette, timeSeriesOption } from "../lib/charts";
import { clear, h } from "../lib/dom";
import { formatBytes, formatDuration, formatMs, formatNumber, formatTime } from "../lib/format";
import { icon } from "../lib/icons";
import { store } from "../lib/store";
import { friendlyError, notify } from "../lib/toast";
import {
  badge,
  button,
  card,
  createCard,
  emptyState,
  keyValues,
  refreshButton,
  statTile,
  table,
} from "../lib/ui";
import { errorNotice, noProcess, pageHead } from "./common";
import type { Cleanup, PageContext, PageDefinition } from "./page";

export function overviewPage(): PageDefinition {
  return {
    path: "/overview",
    title: "Overview",
    description: "A live control room for your Java runtimes",
    group: "Observe",
    icon: "dashboard",
    mount(root, context): Cleanup {
      root.appendChild(
        pageHead(
          "Overview",
          "Live snapshot of the selected JVM, recent garbage collections and the local environment.",
          [
            refreshButton(() => void load()),
            button({ label: "Open monitor", iconName: "activity", variant: "primary", onClick: () => context.navigate("/monitor") }),
          ],
        ),
      );

      const content = h("div", { className: "stack" });
      root.appendChild(content);

      if (!context.pid || !context.process) {
        content.appendChild(noProcess(context));
        content.appendChild(environmentCard());
        return;
      }

      const process = context.process;
      const hero = h(
        "div",
        { className: "hero" },
        h("div", { className: "hero-glyph" }, icon("coffee", 26)),
        h(
          "div",
          {},
          h("h2", {}, process.displayName),
          h("p", {}, `${process.mainClass ?? process.mainJar ?? process.name} · PID ${process.pid} · up ${formatDuration(process.uptimeSecs)}`),
          h(
            "div",
            { className: "hero-tags" },
            badge(process.javaVersion ?? "Java version unknown", "accent"),
            process.vendor ? badge(process.vendor, "neutral") : null,
            process.springPorts.length ? badge(`Spring :${process.springPorts.join(", :")}`, "info") : null,
            process.attachable ? badge("Attachable", "ok") : badge("Attach limited", "warn"),
          ),
        ),
        h(
          "div",
          { className: "hero-actions" },
          button({ label: "Thread dump", iconName: "layers", variant: "secondary", onClick: () => context.navigate("/threads") }),
          button({ label: "Heap dump", iconName: "camera", variant: "secondary", onClick: () => void captureHeap(process.pid, context) }),
          button({ label: "Monitor", iconName: "activity", variant: "primary", onClick: () => context.navigate("/monitor") }),
        ),
      );
      content.appendChild(hero);

      const stats = h("div", { className: "stat-grid" });
      content.appendChild(stats);

      const historyCard = createCard({
        title: "Heap history",
        subtitle: "Last 30 minutes · sampled by jcmd",
        iconName: "activity",
        actions: [button({ label: "Memory details", iconName: "arrowRight", variant: "ghost", size: "sm", onClick: () => context.navigate("/memory") })],
      });
      const chartEl = h("div", { className: "chart chart-md" });
      historyCard.body.appendChild(chartEl);
      content.appendChild(historyCard.root);
      const chart = new ChartHost(chartEl);

      const lower = h("div", { className: "grid-2" });
      const runtimeCard = createCard({ title: "Runtime", subtitle: "From VM.perf properties", iconName: "server" });
      const gcCard = createCard({ title: "Recent collections", subtitle: "Detected deltas in GC counters", iconName: "recycle" });
      lower.appendChild(runtimeCard.root);
      lower.appendChild(gcCard.root);
      content.appendChild(lower);

      const dumpsCard = createCard({ title: "Recent dumps", subtitle: "Stored locally in the JCC data directory", iconName: "database" });
      content.appendChild(dumpsCard.root);

      let disposed = false;
      const pid = process.pid;

      async function load(): Promise<void> {
        try {
          const [sample, samples, gcEvents, dumps] = await Promise.all([
            api.monitorSample(pid),
            api.getSamples(pid, 30, 240),
            api.getGcEvents(pid, 8),
            api.listDumps(),
          ]);
          if (disposed) return;
          renderStats(stats, sample.cpuPercent, sample.heapUsed, sample.heapMax, sample.threadsLive, sample.classesCurrent, sample.gcCount);
          chart.setOption(
            timeSeriesOption(
              [
                { name: "Heap used", color: palette.cyan, area: true, data: samples.map((point) => [point.timestamp, point.heapUsed]) },
                { name: "Committed", color: palette.blue, data: samples.map((point) => [point.timestamp, point.heapCommitted]), dashed: true },
              ],
              { valueFormat: formatBytes, legend: true },
            ),
          );
          clear(runtimeCard.body);
          runtimeCard.body.appendChild(
            keyValues([
              ["Java version", sample.runtime?.javaVersion ?? process.javaVersion ?? "—"],
              ["VM", sample.runtime?.vmName ?? "—"],
              ["Java home", sample.runtime?.javaHome ?? process.javaHome ?? "—"],
              ["Java command", sample.runtime?.javaCommand ?? process.mainClass ?? "—"],
              ["Started", formatTime(process.startTime)],
              ["Source", sample.source],
            ]),
          );
          if (sample.attachError) {
            clear(gcCard.body);
            gcCard.body.appendChild(errorNotice(new Error(sample.attachError), "jcmd attachment issue"));
          } else {
            const view = table(
              [
                { label: "Time", width: "80px" },
                { label: "Collector" },
                { label: "Collections", align: "right" },
                { label: "Pause", align: "right" },
                { label: "Reclaimed", align: "right" },
              ],
              gcEvents.slice(0, 6).map((event) => [
                formatTime(event.timestamp),
                event.collector,
                formatNumber(event.invocations),
                formatMs(event.durationMs),
                formatBytes(event.reclaimedBytes),
              ]),
              { dense: true, empty: "No collections detected since monitoring began" },
            );
            clear(gcCard.body);
            gcCard.body.appendChild(view.wrapper);
          }
          clear(dumpsCard.body);
          if (dumps.length === 0) {
            dumpsCard.body.appendChild(
              emptyState({
                iconName: "database",
                title: "No dumps captured yet",
                description: "Capture a heap dump or start a JFR recording from the process tools.",
              }),
            );
          } else {
            dumpsCard.body.appendChild(table(
              [
                { label: "File" },
                { label: "Kind" },
                { label: "Size", align: "right" },
                { label: "Created", align: "right" },
              ],
              dumps.slice(0, 6).map((dump) => [
                h("span", { className: "mono ellipsis", title: dump.path }, dump.path.split(/[\\/]/).pop() ?? dump.path),
                badge(dump.kind.toUpperCase(), dump.kind === "heap" ? "accent" : "info"),
                formatBytes(dump.sizeBytes),
                formatTime(dump.createdAt),
              ]),
              { dense: true },
            ).wrapper);
          }
        } catch (error) {
          if (!disposed) notify.error("Overview refresh failed", friendlyError(error));
        }
      }

      const timer = window.setInterval(() => void load(), Math.max(4000, store.get().settings.refreshIntervalMs * 2));
      void load();
      return () => {
        disposed = true;
        window.clearInterval(timer);
        chart.dispose();
      };
    },
  };
}

function renderStats(container: HTMLElement, cpu: number, heapUsed: number, heapMax: number, threads: number, classes: number, gcCount: number): void {
  clear(container);
  container.appendChild(
    statTile({ label: "CPU usage", value: `${cpu.toFixed(1)}%`, sub: "process vs system", iconName: "cpu", tone: "info" }),
  );
  container.appendChild(
    statTile({ label: "Heap used", value: formatBytes(heapUsed), sub: `of ${formatBytes(heapMax)}`, iconName: "memory" }),
  );
  container.appendChild(
    statTile({ label: "Live threads", value: formatNumber(threads), sub: "from PerfCounter", iconName: "layers" }),
  );
  container.appendChild(
    statTile({ label: "Loaded classes", value: formatNumber(classes), sub: "currently loaded", iconName: "box" }),
  );
  container.appendChild(
    statTile({ label: "GC collections", value: formatNumber(gcCount), sub: "total invocations", iconName: "recycle", tone: "warn" }),
  );
}

function environmentCard(): HTMLElement {
  const state = store.get();
  return card({
    title: "Environment",
    subtitle: "Detected at startup",
    iconName: "server",
    body: [
      keyValues([
        ["JCC version", state.info?.version ?? "—"],
        ["Platform", `${state.info?.platform ?? "—"} · ${state.info?.arch ?? "—"}`],
        ["Default Java", state.info?.javaHome ?? "none detected"],
        ["Discovered JDKs", String(state.jdks.length)],
        ["Data directory", state.info?.dataDir ?? "—"],
      ]),
    ],
  });
}

async function captureHeap(pid: number, context: PageContext): Promise<void> {
  try {
    const dump = await api.captureHeapDump(pid);
    notify.ok("Heap dump captured", dump.path);
    context.navigate("/heap");
  } catch (error) {
    notify.error("Heap dump failed", friendlyError(error));
  }
}

