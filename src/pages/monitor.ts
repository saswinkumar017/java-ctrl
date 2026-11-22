import { api } from "../lib/api";
import { ChartHost, palette, timeSeriesOption } from "../lib/charts";
import { clear, h } from "../lib/dom";
import { formatBytes, formatMs, formatNumber, percentOf } from "../lib/format";
import { icon } from "../lib/icons";
import { store } from "../lib/store";
import { friendlyError, notify } from "../lib/toast";
import {
  button,
  createCard,
  keyValues,
  liveDot,
  progressBar,
  selectInput,
  spinner,
  statTile,
  table,
} from "../lib/ui";
import { errorNotice, noProcess, pageHead } from "./common";
import type { Cleanup, PageDefinition } from "./page";
import type { MonitorSample, SamplePoint } from "../types";

export function monitorPage(): PageDefinition {
  return {
    path: "/monitor",
    title: "JVM Monitor",
    description: "Live counters from PerfCounter, threads and code cache",
    group: "Observe",
    icon: "activity",
    mount(root, context): Cleanup {
      const liveIndicator = h("span", { className: "flex-row", style: { fontSize: "11px" } }, liveDot("live"), h("span", {}, "Live"));
      const pauseButton = button({
        iconName: "pause",
        label: "Pause",
        variant: "subtle",
        size: "sm",
        onClick: () => {
          paused = !paused;
          clear(pauseButton);
          pauseButton.appendChild(icon(paused ? "play" : "pause", 14));
          pauseButton.appendChild(h("span", {}, paused ? "Resume" : "Pause"));
          liveIndicator.replaceChildren(liveDot(paused ? "idle" : "live"), h("span", {}, paused ? "Paused" : "Live"));
        },
      });
      root.appendChild(
        pageHead(
          "JVM Monitor",
          "Samples are read with jcmd PerfCounter and the OS process table. Nothing is injected into the target JVM.",
          [liveIndicator, pauseButton],
        ),
      );

      if (!context.pid || !context.process) {
        root.appendChild(noProcess(context));
        return;
      }

      const pid = context.pid;
      const content = h("div", { className: "stack" });
      root.appendChild(content);
      const noticeHost = h("div");
      content.appendChild(noticeHost);
      const stats = h("div", { className: "stat-grid" });
      content.appendChild(stats);

      const heapCard = createCard({
        title: "Heap & CPU",
        subtitle: "Heap generations, metaspace and CPU over time",
        iconName: "activity",
        actions: [
          selectInput(
            [
              { value: "5", label: "5 min" },
              { value: "15", label: "15 min" },
              { value: "30", label: "30 min" },
              { value: "120", label: "2 hours" },
            ],
            "30",
            (value) => {
              minutes = Number(value);
              void loadHistory();
              restart();
            },
          ),
        ],
      });
      const heapChartEl = h("div", { className: "chart chart-xl" });
      heapCard.body.appendChild(heapChartEl);
      content.appendChild(heapCard.root);
      const heapChart = new ChartHost(heapChartEl);

      const secondGrid = h("div", { className: "grid-2" });
      const threadCard = createCard({ title: "Threads & classes", subtitle: "Live counts from PerfCounter", iconName: "layers" });
      const threadChartEl = h("div", { className: "chart chart-md" });
      threadCard.body.appendChild(threadChartEl);
      const threadChart = new ChartHost(threadChartEl);

      const poolCard = createCard({ title: "Memory pools", subtitle: "Current committed and used capacity", iconName: "memory" });
      secondGrid.appendChild(threadCard.root);
      secondGrid.appendChild(poolCard.root);
      content.appendChild(secondGrid);

      const detailGrid = h("div", { className: "grid-2" });
      const collectorsCard = createCard({ title: "Garbage collectors", subtitle: "Cumulative invocations and pause time", iconName: "recycle" });
      const runtimeCard = createCard({ title: "Runtime", subtitle: "VM identity and startup flags", iconName: "terminal" });
      detailGrid.appendChild(collectorsCard.root);
      detailGrid.appendChild(runtimeCard.root);
      content.appendChild(detailGrid);

      runtimeCard.body.appendChild(spinner("Reading VM details…"));
      void api.getRuntimeDetails(pid).then((details) => {
        if (disposed) return;
        clear(runtimeCard.body);
        runtimeCard.body.appendChild(
          keyValues([
            ["Java version", details.versionLines[0] ?? "—"],
            ["VM", details.properties["java.vm.name"] ?? "—"],
            ["Vendor", details.properties["java.vendor"] ?? "—"],
            ["Java home", details.properties["java.home"] ?? "—"],
            ["Launcher", details.launcherType ?? "—"],
            ["JVM flags", details.flags.join(" ") || "—"],
            ["Arguments", details.jvmArgs.join(" ") || "—"],
          ]),
        );
      }).catch((error) => {
        if (disposed) return;
        clear(runtimeCard.body);
        runtimeCard.body.appendChild(errorNotice(error, "Runtime details unavailable"));
      });

      let disposed = false;
      let paused = false;
      let minutes = 30;
      let points: SamplePoint[] = [];
      let lastSample: MonitorSample | null = null;
      let timer: number | undefined;

      async function sample(): Promise<void> {
        if (paused) return;
        try {
          const value = await api.monitorSample(pid);
          if (disposed) return;
          lastSample = value;
          const point: SamplePoint = {
            timestamp: value.timestamp,
            cpuPercent: value.cpuPercent,
            heapUsed: value.heapUsed,
            heapCommitted: value.heapCommitted,
            heapMax: value.heapMax,
            nonHeapUsed: value.nonHeapUsed,
            metaspaceUsed: value.metaspaceUsed,
            codeCacheUsed: value.codeCacheUsed,
            threadsLive: value.threadsLive,
            classesCurrent: value.classesCurrent,
            gcCount: value.gcCount,
            gcTimeMs: value.gcTimeMs,
            jitCompiles: value.jitCompiles,
            jitTimeMs: value.jitTimeMs,
          };
          points = [...points.filter((existing) => existing.timestamp !== point.timestamp), point].slice(-600);
          render();
        } catch (error) {
          if (!disposed) notify.error("Monitor sample failed", friendlyError(error));
        }
      }

      async function loadHistory(): Promise<void> {
        try {
          const history = await api.getSamples(pid, minutes, 600);
          if (disposed) return;
          const map = new Map<number, SamplePoint>();
          for (const point of [...history, ...points]) map.set(point.timestamp, point);
          points = Array.from(map.values()).sort((a, b) => a.timestamp - b.timestamp).slice(-600);
          render();
        } catch (error) {
          if (!disposed) notify.error("Could not load history", friendlyError(error));
        }
      }

      function render(): void {
        if (disposed) return;
        clear(noticeHost);
        if (lastSample?.attachError) {
          noticeHost.appendChild(errorNotice(new Error(lastSample.attachError), "PerfCounter attachment issue"));
        }
        renderStats();
        heapChart.setOption(
          timeSeriesOption(
            [
              { name: "Heap used", color: palette.cyan, area: true, data: points.map((point) => [point.timestamp, point.heapUsed]) },
              { name: "Committed", color: palette.blue, data: points.map((point) => [point.timestamp, point.heapCommitted]), dashed: true },
              { name: "Metaspace", color: palette.violet, data: points.map((point) => [point.timestamp, point.metaspaceUsed]) },
              { name: "CPU", color: palette.amber, yAxisIndex: 1, data: points.map((point) => [point.timestamp, point.cpuPercent]) },
            ],
            { valueFormat: formatBytes, yName: "Memory" },
          ),
        );
        threadChart.setOption(
          timeSeriesOption(
            [
              { name: "Threads", color: palette.green, data: points.map((point) => [point.timestamp, point.threadsLive]), area: true },
              { name: "Classes", color: palette.pink, data: points.map((point) => [point.timestamp, point.classesCurrent]) },
            ],
            { valueFormat: (value) => formatNumber(value), yName: "Count", gridLeft: 52 },
          ),
        );
        renderPools();
        renderCollectors();
      }

      function renderStats(): void {
        clear(stats);
        const sample = lastSample;
        stats.appendChild(statTile({ label: "CPU", value: sample ? `${sample.cpuPercent.toFixed(1)}%` : "—", sub: "process usage", iconName: "cpu", tone: "info" }));
        stats.appendChild(statTile({
          label: "Heap used",
          value: sample ? formatBytes(sample.heapUsed) : "—",
          sub: sample ? `${percentOf(sample.heapUsed, sample.heapMax).toFixed(1)}% of ${formatBytes(sample.heapMax)}` : "—",
          iconName: "memory",
          tone: sample && percentOf(sample.heapUsed, sample.heapMax) > 85 ? "danger" : "accent",
        }));
        stats.appendChild(statTile({ label: "Threads", value: sample ? formatNumber(sample.threadsLive) : "—", sub: sample ? `peak ${formatNumber(sample.threadsPeak)}` : "—", iconName: "layers", tone: "ok" }));
        stats.appendChild(statTile({ label: "Classes", value: sample ? formatNumber(sample.classesCurrent) : "—", sub: sample ? `${formatNumber(sample.classesLoaded)} loaded` : "—", iconName: "box" }));
        stats.appendChild(statTile({ label: "GC time", value: sample ? formatMs(sample.gcTimeMs) : "—", sub: sample ? `${formatNumber(sample.gcCount)} collections` : "—", iconName: "recycle", tone: "warn" }));
        stats.appendChild(statTile({ label: "JIT compiles", value: sample ? formatNumber(sample.jitCompiles) : "—", sub: sample ? `${formatNumber(sample.jitNmethods)} nmethods` : "—", iconName: "zap", tone: "info" }));
      }

      function renderPools(): void {
        clear(poolCard.body);
        const pools = lastSample?.memoryPools ?? [];
        if (pools.length === 0) {
          poolCard.body.appendChild(h("p", { className: "muted" }, "No memory pool information in this sample."));
          return;
        }
        for (const pool of pools) {
          poolCard.body.appendChild(
            h(
              "div",
              { style: { marginBottom: "11px" } },
              h(
                "div",
                { className: "flex-row", style: { justifyContent: "space-between", marginBottom: "3px" } },
                h("span", {}, pool.name),
                h("span", { className: "mono dim" }, `${formatBytes(pool.used)} / ${formatBytes(pool.max)}`),
              ),
              progressBar(pool.used, pool.max || pool.committed, percentOf(pool.used, pool.max) > 85 ? "danger" : "accent"),
              h("div", { className: "cell-sub" }, `${pool.kind} · committed ${formatBytes(pool.committed)}`),
            ),
          );
        }
      }

      function renderCollectors(): void {
        clear(collectorsCard.body);
        const collectors = lastSample?.gcCollectors ?? [];
        collectorsCard.body.appendChild(
          table(
            [
              { label: "Collector" },
              { label: "Invocations", align: "right" },
              { label: "Total time", align: "right" },
              { label: "Last pause", align: "right" },
            ],
            collectors.map((collector) => [
              collector.name,
              formatNumber(collector.invocations),
              formatMs(collector.timeMs),
              formatMs(collector.lastPauseMs),
            ]),
            { dense: true, empty: "No collector counters available" },
          ).wrapper,
        );
        if (lastSample?.gcLastCause) {
          collectorsCard.body.appendChild(
            h("div", { className: "notice notice-info", style: { margin: "12px 16px 0" } }, icon("info", 14), h("span", {}, `Last GC cause: ${lastSample.gcLastCause}`)),
          );
        }
      }

      function restart(): void {
        if (timer != null) window.clearInterval(timer);
        timer = window.setInterval(() => void sample(), Math.max(500, store.get().settings.refreshIntervalMs));
      }

      void loadHistory();
      void sample();
      restart();

      return () => {
        disposed = true;
        if (timer != null) window.clearInterval(timer);
        heapChart.dispose();
        threadChart.dispose();
      };
    },
  };
}

