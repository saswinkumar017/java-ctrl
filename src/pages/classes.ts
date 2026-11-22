import { api } from "../lib/api";
import { ChartHost, horizontalBarOption, palette } from "../lib/charts";
import { clear, h } from "../lib/dom";
import { formatBytes, formatNumber } from "../lib/format";
import { createCard, refreshButton, searchInput, spinner, statTile, table } from "../lib/ui";
import { infoNotice, noProcess, pageHead } from "./common";
import type { Cleanup, PageDefinition } from "./page";
import type { ClassLoadingSnapshot } from "../types";

export function classesPage(): PageDefinition {
  return {
    path: "/classes",
    title: "Class Loading",
    description: "Class loaders and class histogram",
    group: "Observe",
    icon: "box",
    mount(root, context): Cleanup {
      root.appendChild(
        pageHead(
          "Class Loading",
          "Reads VM.classloader_stats and GC.class_histogram. The histogram may briefly safepoint the target JVM.",
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
      const noticeHost = h("div");
      content.appendChild(noticeHost);
      const stats = h("div", { className: "stat-grid" });
      content.appendChild(stats);
      stats.appendChild(spinner("Reading class loader statistics…"));

      const chartCard = createCard({ title: "Largest classes", subtitle: "Top 15 by retained bytes in the histogram", iconName: "box" });
      const chartEl = h("div", { className: "chart chart-lg" });
      chartCard.body.appendChild(chartEl);
      content.appendChild(chartCard.root);
      const chart = new ChartHost(chartEl);

      const grid = h("div", { className: "grid-2" });
      const loaderCard = createCard({ title: "Class loaders", subtitle: "From VM.classloader_stats", iconName: "layers" });
      const histogramCard = createCard({ title: "Class histogram", subtitle: "Instances and bytes per class", iconName: "database" });
      grid.appendChild(loaderCard.root);
      grid.appendChild(histogramCard.root);
      content.appendChild(grid);
      const search = searchInput("Filter classes…", (value) => {
        filter = value;
        renderHistogram();
      });
      histogramCard.header?.appendChild(search.wrapper);

      let disposed = false;
      let snapshot: ClassLoadingSnapshot | null = null;
      let filter = "";

      async function load(): Promise<void> {
        try {
          const value = await api.getClassLoading(pid);
          if (disposed) return;
          snapshot = value;
          render(value);
        } catch (error) {
          if (disposed) return;
          clear(stats);
          stats.appendChild(
            h("div", { className: "notice notice-error" }, `Could not read class statistics: ${error instanceof Error ? error.message : String(error)}`),
          );
        }
      }

      function render(value: ClassLoadingSnapshot): void {
        clear(noticeHost);
        if (value.notes.length) noticeHost.appendChild(infoNotice(value.notes.join(" ")));
        renderStats(value);
        chart.setOption(
          horizontalBarOption(
            value.histogram.slice(0, 15).map((row) => ({
              name: row.className.split("/").pop() ?? row.className,
              value: row.bytes,
              color: palette.violet,
            })),
            { valueFormat: formatBytes },
          ),
        );
        renderLoaders(value);
        renderHistogram();
      }

      function renderStats(value: ClassLoadingSnapshot): void {
        clear(stats);
        stats.appendChild(statTile({ label: "Classes loaded", value: formatNumber(value.totalClasses), sub: "from classloader stats", iconName: "box", tone: "accent" }));
        stats.appendChild(statTile({ label: "Class loaders", value: formatNumber(value.loaders.length), sub: `${formatNumber(value.loaders.reduce((sum, loader) => sum + loader.hiddenClasses, 0))} hidden classes`, iconName: "layers" }));
        stats.appendChild(statTile({ label: "Histogram instances", value: formatNumber(value.totalInstances), sub: `${formatNumber(value.histogram.length)} class rows`, iconName: "database", tone: "info" }));
        stats.appendChild(statTile({ label: "Histogram bytes", value: formatBytes(value.totalBytes), sub: "live instances", iconName: "memory", tone: "warn" }));
      }

      function renderLoaders(value: ClassLoadingSnapshot): void {
        clear(loaderCard.body);
        loaderCard.body.appendChild(
          table(
            [
              { label: "ID" },
              { label: "Parent" },
              { label: "Kind" },
              { label: "Classes", align: "right" },
              { label: "Chunk", align: "right" },
              { label: "Block", align: "right" },
              { label: "CLD" },
            ],
            value.loaders.map((loader) => [
              h("span", { className: "mono" }, loader.id),
              h("span", { className: "mono dim" }, loader.parentId),
              loader.kind,
              formatNumber(loader.classes),
              formatBytes(loader.chunkSize),
              formatBytes(loader.blockSize),
              h("span", { className: "mono dim" }, loader.cld),
            ]),
            { dense: true, empty: "No loader information available" },
          ).wrapper,
        );
      }

      function renderHistogram(): void {
        if (!snapshot) return;
        clear(histogramCard.body);
        const rows = snapshot.histogram.filter((row) =>
          !filter || row.className.toLowerCase().includes(filter) || (row.module ?? "").toLowerCase().includes(filter),
        );
        histogramCard.body.appendChild(
          table(
            [
              { label: "#", width: "44px" },
              { label: "Class" },
              { label: "Instances", align: "right" },
              { label: "Bytes", align: "right" },
              { label: "Module" },
            ],
            rows.slice(0, 200).map((row) => [
              String(row.rank),
              h("span", { className: "mono ellipsis", title: row.className }, row.className),
              formatNumber(row.instances),
              formatBytes(row.bytes),
              h("span", { className: "dim" }, row.module ?? "—"),
            ]),
            { dense: true, empty: "No histogram rows match the filter" },
          ).wrapper,
        );
      }

      void load();
      return () => {
        disposed = true;
        chart.dispose();
      };
    },
  };
}
