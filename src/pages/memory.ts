import { api } from "../lib/api";
import { ChartHost, donutOption, palette, timeSeriesOption } from "../lib/charts";
import { clear, h } from "../lib/dom";
import { formatBytes, formatNumber, percentOf } from "../lib/format";
import { store } from "../lib/store";
import { refreshButton, createCard, statTile, table, progressBar, spinner, badge } from "../lib/ui";
import { errorNotice, noProcess, pageHead } from "./common";
import type { Cleanup, PageDefinition } from "./page";
import type { MonitorSample } from "../types";

export function memoryPage(): PageDefinition {
  return {
    path: "/memory",
    title: "Memory Explorer",
    description: "Heap generations, metaspace and code cache",
    group: "Observe",
    icon: "memory",
    mount(root, context): Cleanup {
      root.appendChild(
        pageHead(
          "Memory Explorer",
          "Heap generation sizes come from PerfCounter; the pool table is the JVM's own memory pool view.",
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
      stats.appendChild(spinner("Sampling memory counters…"));

      const chartCard = createCard({
        title: "Memory history",
        subtitle: "Heap, committed heap, metaspace and code cache",
        iconName: "activity",
      });
      const chartEl = h("div", { className: "chart chart-lg" });
      chartCard.body.appendChild(chartEl);
      content.appendChild(chartCard.root);
      const chart = new ChartHost(chartEl);

      const grid = h("div", { className: "grid-2" });
      const compositionCard = createCard({ title: "Heap composition", subtitle: "Current generation usage", iconName: "layers" });
      const poolCard = createCard({ title: "Memory pools", subtitle: "Used / committed / maximum", iconName: "memory" });
      grid.appendChild(compositionCard.root);
      grid.appendChild(poolCard.root);
      content.appendChild(grid);

      const detailCard = createCard({ title: "Non-heap breakdown", subtitle: "Metaspace, compressed class space and code cache", iconName: "server" });
      content.appendChild(detailCard.root);

      let disposed = false;
      let compositionChart: ChartHost | null = null;

      async function load(): Promise<void> {
        try {
          const [current, history] = await Promise.all([api.monitorSample(pid), api.getSamples(pid, 30, 400)]);
          if (disposed) return;
          renderStats(current);
          chart.setOption(
            timeSeriesOption(
              [
                { name: "Heap used", color: palette.cyan, area: true, data: history.map((point) => [point.timestamp, point.heapUsed]) },
                { name: "Committed", color: palette.blue, data: history.map((point) => [point.timestamp, point.heapCommitted]), dashed: true },
                { name: "Metaspace", color: palette.violet, data: history.map((point) => [point.timestamp, point.metaspaceUsed]) },
                { name: "Code cache", color: palette.amber, data: history.map((point) => [point.timestamp, point.codeCacheUsed]) },
              ],
              { valueFormat: formatBytes, yName: "Bytes" },
            ),
          );
          renderComposition(current);
          renderPools(current);
          renderNonHeap(current);
        } catch (error) {
          if (disposed) return;
          clear(stats);
          stats.appendChild(errorNotice(error, "Memory snapshot failed"));
        }
      }

      function renderStats(current: MonitorSample): void {
        clear(stats);
        stats.appendChild(statTile({
          label: "Heap used",
          value: formatBytes(current.heapUsed),
          sub: `${percentOf(current.heapUsed, current.heapMax).toFixed(1)}% of ${formatBytes(current.heapMax)}`,
          iconName: "memory",
          tone: percentOf(current.heapUsed, current.heapMax) > 85 ? "danger" : "accent",
        }));
        stats.appendChild(statTile({ label: "Eden", value: formatBytes(current.edenUsed), sub: `committed ${formatBytes(current.edenCommitted)}`, iconName: "layers" }));
        stats.appendChild(statTile({ label: "Survivor", value: formatBytes(current.survivorUsed), sub: `committed ${formatBytes(current.survivorCommitted)}`, iconName: "layers", tone: "info" }));
        stats.appendChild(statTile({ label: "Old generation", value: formatBytes(current.oldUsed), sub: `committed ${formatBytes(current.oldCommitted)}`, iconName: "database", tone: "warn" }));
        stats.appendChild(statTile({ label: "Metaspace", value: formatBytes(current.metaspaceUsed), sub: `committed ${formatBytes(current.metaspaceCommitted)}`, iconName: "box", tone: "info" }));
        stats.appendChild(statTile({ label: "Code cache", value: formatBytes(current.codeCacheUsed), sub: `${formatNumber(current.jitNmethods)} nmethods`, iconName: "zap" }));
      }

      function renderComposition(current: MonitorSample): void {
        clear(compositionCard.body);
        if (compositionChart) {
          compositionChart.dispose();
          compositionChart = null;
        }
        const used = current.heapUsed;
        const known = current.edenUsed + current.survivorUsed + current.oldUsed;
        const items = [
          { name: "Eden", value: current.edenUsed },
          { name: "Survivor", value: current.survivorUsed },
          { name: "Old gen", value: current.oldUsed },
        ];
        if (used > known) items.push({ name: "Other / unaccounted", value: used - known });
        const element = h("div", { className: "chart chart-md" });
        compositionCard.body.appendChild(element);
        compositionChart = new ChartHost(element);
        compositionChart.setOption(donutOption(items, [palette.cyan, palette.blue, palette.violet, palette.slate]));
        compositionCard.body.appendChild(
          h("p", { className: "muted", style: { fontSize: "10.5px", margin: "4px 0 0" } }, "Shares of the current heap used value. Generation counters are exposed by PerfCounter."),
        );
      }

      function renderPools(current: MonitorSample): void {
        clear(poolCard.body);
        poolCard.body.appendChild(
          table(
            [
              { label: "Pool" },
              { label: "Type" },
              { label: "Used", align: "right" },
              { label: "Committed", align: "right" },
              { label: "Max", align: "right" },
              { label: "Usage", width: "90px" },
            ],
            current.memoryPools.map((pool) => [
              h("span", {}, pool.name),
              badge(pool.kind, pool.kind.toLowerCase().includes("heap") && !pool.kind.toLowerCase().includes("non") ? "accent" : "neutral"),
              formatBytes(pool.used),
              formatBytes(pool.committed),
              pool.max ? formatBytes(pool.max) : "unbounded",
              progressBar(pool.used, pool.max || pool.committed, percentOf(pool.used, pool.max || pool.committed) > 85 ? "danger" : "accent"),
            ]),
            { dense: true, empty: "No memory pools reported" },
          ).wrapper,
        );
      }

      function renderNonHeap(current: MonitorSample): void {
        clear(detailCard.body);
        const items: Array<[string, number, number]> = [
          ["Metaspace", current.metaspaceUsed, current.metaspaceMax],
          ["Compressed class space", current.compressedClassSpaceUsed, current.compressedClassSpaceMax],
          ["Code cache", current.codeCacheUsed, current.codeCacheMax],
          ["Total non-heap", current.nonHeapUsed, 0],
        ];
        detailCard.body.appendChild(
          h(
            "div",
            {},
            ...items.map(([label, used, max]) =>
              h(
                "div",
                { style: { marginBottom: "12px" } },
                h(
                  "div",
                  { className: "flex-row", style: { justifyContent: "space-between" } },
                  h("span", {}, label),
                  h("span", { className: "mono dim" }, max ? `${formatBytes(used)} / ${formatBytes(max)}` : formatBytes(used)),
                ),
                max ? progressBar(used, max, percentOf(used, max) > 85 ? "danger" : "info") : null,
              ),
            ),
          ),
        );
      }

      void load();
      const timer = window.setInterval(() => {
        if (!document.hidden) void load();
      }, Math.max(6000, store.get().settings.refreshIntervalMs * 3));

      return () => {
        disposed = true;
        window.clearInterval(timer);
        chart.dispose();
        compositionChart?.dispose();
      };
    },
  };
}
