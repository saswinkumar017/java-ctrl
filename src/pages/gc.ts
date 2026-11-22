import { api } from "../lib/api";
import { ChartHost, palette, timeSeriesOption } from "../lib/charts";
import { clear, h } from "../lib/dom";
import { formatBytes, formatMs, formatNumber, formatTime } from "../lib/format";
import { icon } from "../lib/icons";
import { store } from "../lib/store";
import { friendlyError, notify } from "../lib/toast";
import { createCard, refreshButton, statTile, table } from "../lib/ui";
import { infoNotice, noProcess, pageHead } from "./common";
import type { Cleanup, PageDefinition } from "./page";
import type { GcEvent, MonitorSample } from "../types";

export function gcPage(): PageDefinition {
  return {
    path: "/gc",
    title: "GC Monitor",
    description: "Collection counts, pause times and reclaimed memory",
    group: "Observe",
    icon: "recycle",
    mount(root, context): Cleanup {
      root.appendChild(
        pageHead(
          "GC Monitor",
          "Collection events are detected by watching PerfCounter deltas between samples and stored locally for trend analysis.",
          [refreshButton(() => void load())],
        ),
      );
      if (!context.pid) {
        root.appendChild(noProcess(context));
        return;
      }

      const pid = context.pid;
      const content = h("div", { className: "stack" });
      root.appendChild(content);
      const stats = h("div", { className: "stat-grid" });
      content.appendChild(stats);

      const charts = h("div", { className: "grid-2" });
      const countCard = createCard({ title: "Collection count", subtitle: "Cumulative invocations", iconName: "recycle" });
      const timeCard = createCard({ title: "Cumulative pause time", subtitle: "Total time spent in GC", iconName: "clock" });
      const countEl = h("div", { className: "chart chart-md" });
      const timeEl = h("div", { className: "chart chart-md" });
      countCard.body.appendChild(countEl);
      timeCard.body.appendChild(timeEl);
      charts.appendChild(countCard.root);
      charts.appendChild(timeCard.root);
      content.appendChild(charts);
      const countChart = new ChartHost(countEl);
      const timeChart = new ChartHost(timeEl);

      const eventsCard = createCard({ title: "Detected collections", subtitle: "Stored GC events for this process", iconName: "database" });
      content.appendChild(eventsCard.root);
      const collectorsCard = createCard({ title: "Collector counters", subtitle: "Current cumulative values from PerfCounter", iconName: "cpu" });
      content.appendChild(collectorsCard.root);

      let disposed = false;
      let latest: MonitorSample | null = null;
      let timer: number | undefined;

      async function load(): Promise<void> {
        try {
          const [sample, events, history] = await Promise.all([
            api.monitorSample(pid),
            api.getGcEvents(pid, 200),
            api.getSamples(pid, 30, 600),
          ]);
          if (disposed) return;
          latest = sample;
          renderStats(sample, events);
          countChart.setOption(timeSeriesOption(
            [{ name: "Collections", color: palette.amber, step: "end", area: true, data: history.map((point) => [point.timestamp, point.gcCount]) }],
            { valueFormat: (value) => formatNumber(value), yName: "Count", legend: false, gridLeft: 56 },
          ));
          timeChart.setOption(timeSeriesOption(
            [{ name: "Pause time", color: palette.red, step: "end", area: true, data: history.map((point) => [point.timestamp, point.gcTimeMs]) }],
            { valueFormat: formatMs, yName: "Time", legend: false, gridLeft: 62 },
          ));
          renderEvents(events);
          renderCollectors(sample);
        } catch (error) {
          if (!disposed) notify.error("GC refresh failed", friendlyError(error));
        }
      }

      function renderStats(sample: MonitorSample, events: GcEvent[]): void {
        clear(stats);
        stats.appendChild(statTile({ label: "Collections", value: formatNumber(sample.gcCount), sub: "total invocations", iconName: "recycle", tone: "warn" }));
        stats.appendChild(statTile({ label: "Total GC time", value: formatMs(sample.gcTimeMs), sub: "cumulative pause time", iconName: "clock", tone: "danger" }));
        stats.appendChild(statTile({
          label: "Avg pause",
          value: sample.gcCount ? formatMs(sample.gcTimeMs / sample.gcCount) : "—",
          sub: "across all collections",
          iconName: "activity",
        }));
        stats.appendChild(statTile({
          label: "Reclaimed",
          value: formatBytes(events.reduce((sum, event) => sum + event.reclaimedBytes, 0)),
          sub: `from ${formatNumber(events.length)} tracked events`,
          iconName: "memory",
          tone: "ok",
        }));
        stats.appendChild(statTile({
          label: "Last cause",
          value: sample.gcLastCause ?? "—",
          sub: "reported by the JVM",
          iconName: "info",
          tone: "info",
        }));
      }

      function renderEvents(events: GcEvent[]): void {
        clear(eventsCard.body);
        if (latest?.attachError) {
          eventsCard.body.appendChild(infoNotice(latest.attachError));
        }
        eventsCard.body.appendChild(
          table(
            [
              { label: "Time", width: "84px" },
              { label: "Collector" },
              { label: "Collections", align: "right" },
              { label: "Pause", align: "right" },
              { label: "Before", align: "right" },
              { label: "After", align: "right" },
              { label: "Reclaimed", align: "right" },
              { label: "Cause" },
            ],
            events.slice(0, 120).map((event) => [
              formatTime(event.timestamp),
              event.collector,
              formatNumber(event.invocations),
              formatMs(event.durationMs),
              formatBytes(event.heapBefore),
              formatBytes(event.heapAfter),
              h("span", { className: "text-ok" }, formatBytes(event.reclaimedBytes)),
              event.cause ?? "—",
            ]),
            { dense: true, empty: "No collection events detected yet. Keep JCC open while the JVM allocates." },
          ).wrapper,
        );
        eventsCard.body.appendChild(
          h("p", { className: "muted", style: { fontSize: "10.5px", margin: "10px 14px 0" } },
            "Events are only produced while JCC is sampling. GC logs and JFR recordings provide complete histories for longer-running JVMs."),
        );
      }

      function renderCollectors(sample: MonitorSample): void {
        clear(collectorsCard.body);
        collectorsCard.body.appendChild(
          table(
            [
              { label: "#", width: "44px" },
              { label: "Collector" },
              { label: "Invocations", align: "right" },
              { label: "Time", align: "right" },
              { label: "Last pause", align: "right" },
              { label: "Share", width: "120px" },
            ],
            sample.gcCollectors.map((collector) => [
              String(collector.index),
              collector.name,
              formatNumber(collector.invocations),
              formatMs(collector.timeMs),
              formatMs(collector.lastPauseMs),
              h("span", { className: "dim" }, sample.gcTimeMs ? `${((collector.timeMs / sample.gcTimeMs) * 100).toFixed(1)}%` : "—"),
            ]),
            { dense: true, empty: "No collector data in this sample" },
          ).wrapper,
        );
        if (sample.gcCollectors.length > 1) {
          collectorsCard.body.appendChild(
            h("div", { className: "flex-row", style: { padding: "12px 14px" } }, icon("info", 14),
              h("span", { className: "muted" }, "Multiple collectors report cumulative counters; the total may include nested collection phases.")),
          );
        }
      }

      void load();
      timer = window.setInterval(() => {
        if (!document.hidden) void load();
      }, Math.max(5000, store.get().settings.refreshIntervalMs * 3));

      return () => {
        disposed = true;
        if (timer != null) window.clearInterval(timer);
        countChart.dispose();
        timeChart.dispose();
      };
    },
  };
}
