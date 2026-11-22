import { api, copyToClipboard, pickHprofFile } from "../lib/api";
import { ChartHost, donutOption, horizontalBarOption, palette } from "../lib/charts";
import { clear, h } from "../lib/dom";
import { formatBytes, formatDateTime, formatMs, formatNumber } from "../lib/format";
import { friendlyError, notify } from "../lib/toast";
import {
  badge,
  button,
  checkbox,
  confirmAction,
  createCard,
  emptyState,
  iconButton,
  refreshButton,
  spinner,
  statTile,
  table,
} from "../lib/ui";
import { noProcess, pageHead } from "./common";
import type { Cleanup, PageContext, PageDefinition } from "./page";
import type { DumpInfo, HeapAnalysis, HeapObjectRow } from "../types";

export function heapPage(): PageDefinition {
  return {
    path: "/heap",
    title: "Heap Analyzer",
    description: "HPROF parsing, reachability and dominators",
    group: "Inspect",
    icon: "database",
    mount(root, context): Cleanup {
      root.appendChild(
        pageHead(
          "Heap Analyzer",
          "Parses HPROF files in-process and computes reachability, weak-reference closure and retained sizes.",
          [
            refreshButton(() => void loadDumps()),
            button({ label: "Open .hprof", iconName: "folder", variant: "subtle", onClick: () => void openExternal() }),
            button({
              label: "Capture heap dump",
              iconName: "camera",
              variant: "primary",
              disabled: !context.pid,
              title: context.pid ? undefined : "Select a JVM process first",
              onClick: () => void capture(context),
            }),
          ],
        ),
      );

      if (!context.pid) {
        root.appendChild(noProcess(context, "Select a JVM process to capture a new heap dump. You can still open and analyze existing .hprof files."));
      }

      const layout = h("div", { className: "split-layout" });
      root.appendChild(layout);
      const dumpCard = createCard({
        title: "Captured dumps",
        subtitle: "Stored in the JCC data directory",
        iconName: "database",
      });
      layout.appendChild(dumpCard.root);
      const results = h("div", { className: "stack" });
      layout.appendChild(results);

      dumpCard.body.appendChild(spinner("Loading dumps…"));
      const resultsCard = createCard({
        title: "Analysis",
        subtitle: "Reachability, retained sizes and object details",
        iconName: "layers",
      });
      results.appendChild(resultsCard.root);
      resultsCard.body.appendChild(
        emptyState({
          iconName: "database",
          title: "No dump analysed yet",
          description: "Capture a heap dump from a running JVM or open an existing HPROF file to inspect object graphs and retained sizes.",
        }),
      );

      let disposed = false;
      let activeTab = "dominators";
      let current: HeapAnalysis | null = null;
      const charts: ChartHost[] = [];

      async function loadDumps(): Promise<void> {
        try {
          const dumps = await api.listDumps();
          if (disposed) return;
          renderDumps(dumps);
        } catch (error) {
          if (disposed) return;
          clear(dumpCard.body);
          dumpCard.body.appendChild(h("p", { className: "notice notice-error" }, friendlyError(error)));
        }
      }

      function renderDumps(dumps: DumpInfo[]): void {
        clear(dumpCard.body);
        if (dumps.length === 0) {
          dumpCard.body.appendChild(
            emptyState({
              iconName: "database",
              title: "No dumps yet",
              description: "Use Capture heap dump on a selected JVM, then analyse it here.",
            }),
          );
          return;
        }
        for (const dump of dumps) {
          const fileName = dump.path.split(/[\\/]/).pop() ?? dump.path;
          dumpCard.body.appendChild(
            h(
              "div",
              { className: "dump-row" },
              h("div", { className: ["dump-icon", dump.kind === "heap" ? "heap" : ""].filter(Boolean) },
                dump.kind === "heap" ? "▣" : "◷"),
              h(
                "div",
                { className: "dump-main" },
                h("div", { className: "dump-name", title: dump.path }, fileName),
                h("div", { className: "dump-meta" }, `${formatBytes(dump.sizeBytes)} · ${formatDateTime(dump.createdAt)}`),
                h("div", { className: "flex-row", style: { marginTop: "6px" } },
                  badge(dump.kind.toUpperCase(), dump.kind === "heap" ? "accent" : "info"),
                  dump.pid ? h("span", { className: "cell-sub" }, `PID ${dump.pid}`) : null,
                ),
              ),
              h(
                "div",
                { className: "flex-row", style: { flexDirection: "column", gap: "4px" } },
                dump.kind === "heap"
                  ? iconButton("activity", "Analyse dump", () => void analyse(dump.path))
                  : null,
                iconButton("copy", "Copy path", () => void copyToClipboard(dump.path).then(() => notify.ok("Path copied"))),
                iconButton("trash", "Delete dump", () => void removeDump(dump)),
              ),
            ),
          );
        }
      }

      async function analyse(path: string): Promise<void> {
        clear(resultsCard.body);
        resultsCard.body.appendChild(spinner("Parsing HPROF, computing reachability and dominators…"));
        try {
          const analysis = await api.analyzeHeap(path);
          if (disposed) return;
          current = analysis;
          renderAnalysis(analysis);
          notify.ok("Heap analysis complete", `${formatNumber(analysis.totalObjects)} objects in ${formatMs(analysis.analysisMs)}`);
        } catch (error) {
          if (disposed) return;
          clear(resultsCard.body);
          resultsCard.body.appendChild(h("p", { className: "notice notice-error" }, friendlyError(error)));
        }
      }

      function renderAnalysis(analysis: HeapAnalysis): void {
        for (const chart of charts) chart.dispose();
        charts.length = 0;
        clear(resultsCard.body);
        const stats = h("div", { className: "stat-grid" });
        resultsCard.body.appendChild(stats);
        stats.appendChild(statTile({ label: "Objects", value: formatNumber(analysis.totalObjects), sub: `${formatNumber(analysis.classCount)} classes`, iconName: "database", tone: "accent" }));
        stats.appendChild(statTile({ label: "Reachable", value: formatNumber(analysis.reachableObjects), sub: formatBytes(analysis.reachableBytes), iconName: "check", tone: "ok" }));
        stats.appendChild(statTile({ label: "Weak-only", value: formatNumber(analysis.weakObjects), sub: formatBytes(analysis.weakBytes), iconName: "shield", tone: "warn" }));
        stats.appendChild(statTile({ label: "Shallow size", value: formatBytes(analysis.totalShallowBytes), sub: `file ${formatBytes(analysis.fileSize)}`, iconName: "memory", tone: "info" }));
        stats.appendChild(statTile({
          label: "Analysis time",
          value: formatMs(analysis.analysisMs),
          sub: analysis.dominatorsComputed ? "dominators computed" : "dominators skipped",
          iconName: "clock",
          tone: analysis.dominatorsComputed ? "ok" : "warn",
        }));

        resultsCard.body.appendChild(
          h("div", { className: "notice notice-info" },
            h("span", {}, h("strong", {}, analysis.path), h("br", {}), `ID size ${analysis.idSize} bytes · parsed from HPROF binary format · weak references are tracked separately from strong reachability.`)),
        );
        if (analysis.notes.length) {
          resultsCard.body.appendChild(
            h("div", { className: "notice notice-warn" }, ...analysis.notes.map((note) => h("div", {}, note))),
          );
        }

        const visual = h("div", { className: "grid-2" });
        const classCard = createCard({ title: "Classes by shallow size", subtitle: "Top 15", iconName: "box" });
        const rootCard = createCard({ title: "GC roots", subtitle: "By root kind", iconName: "shield" });
        const classEl = h("div", { className: "chart chart-md" });
        const rootEl = h("div", { className: "chart chart-md" });
        classCard.body.appendChild(classEl);
        rootCard.body.appendChild(rootEl);
        visual.appendChild(classCard.root);
        visual.appendChild(rootCard.root);
        resultsCard.body.appendChild(visual);
        const classChart = new ChartHost(classEl);
        const rootChart = new ChartHost(rootEl);
        charts.push(classChart, rootChart);
        classChart.setOption(horizontalBarOption(
          analysis.classes.slice(0, 15).map((row) => ({ name: row.className, value: row.bytes, color: palette.violet })),
          { valueFormat: formatBytes },
        ));
        rootChart.setOption(donutOption(
          analysis.rootCounts.map((row) => ({ name: row.kind, value: row.count })),
          [palette.cyan, palette.blue, palette.amber, palette.violet, palette.pink, palette.slate],
        ));

        const tabs = h("div", { className: "tabs" });
        const contentHost = h("div");
        resultsCard.body.appendChild(tabs);
        resultsCard.body.appendChild(contentHost);
        const tabDefs = [
          { id: "dominators", label: `Dominators (${analysis.dominators.length})` },
          { id: "top", label: `Top objects (${analysis.topObjects.length})` },
          { id: "classes", label: `Classes (${analysis.classes.length})` },
          { id: "weak", label: `Weak classes (${analysis.weakClasses.length})` },
          { id: "loaders", label: `Class loaders (${analysis.loaders.length})` },
        ];
        for (const tab of tabDefs) {
          tabs.appendChild(
            h("button", {
              className: tab.id === activeTab ? "active" : "",
              onClick: () => {
                activeTab = tab.id;
                renderTabs();
              },
            }, tab.label),
          );
        }

        function renderTabs(): void {
          for (const [index, child] of Array.from(tabs.children).entries()) {
            child.classList.toggle("active", tabDefs[index]?.id === activeTab);
          }
          clear(contentHost);
          if (activeTab === "dominators" || activeTab === "top") {
            const rows = activeTab === "dominators" ? analysis.dominators : analysis.topObjects;
            contentHost.appendChild(
              table(
                [
                  { label: "#", width: "48px" },
                  { label: "Object ID" },
                  { label: "Class" },
                  { label: "Shallow", align: "right" },
                  { label: "Retained", align: "right" },
                ],
                rows.slice(0, 300).map((row: HeapObjectRow, index) => [
                  String(index + 1),
                  h("span", { className: "mono dim" }, `0x${row.objectId}`),
                  h("span", { className: "mono" }, row.className),
                  formatBytes(row.shallowBytes),
                  row.retainedBytes == null
                    ? h("span", { className: "muted" }, "—")
                    : h("span", { className: "text-accent" }, formatBytes(row.retainedBytes)),
                ]),
                { dense: true, empty: "No objects in this view" },
              ).wrapper,
            );
            if (activeTab === "dominators" && !analysis.dominatorsComputed) {
              contentHost.appendChild(h("p", { className: "muted" }, "Dominator computation is skipped for very large dumps to keep analysis interactive."));
            }
          } else if (activeTab === "classes" || activeTab === "weak") {
            const rows = activeTab === "classes" ? analysis.classes : analysis.weakClasses;
            contentHost.appendChild(
              table(
                [
                  { label: "#", width: "48px" },
                  { label: "Class" },
                  { label: "Instances", align: "right" },
                  { label: "Bytes", align: "right" },
                ],
                rows.slice(0, 300).map((row, index) => [
                  String(index + 1),
                  h("span", { className: "mono" }, row.className),
                  formatNumber(row.instances),
                  formatBytes(row.bytes),
                ]),
                { dense: true, empty: "No class rows" },
              ).wrapper,
            );
          } else {
            contentHost.appendChild(
              table(
                [
                  { label: "Class loader" },
                  { label: "Objects", align: "right" },
                  { label: "Bytes", align: "right" },
                  { label: "Classes", align: "right" },
                ],
                analysis.loaders.map((row) => [
                  h("span", { className: "mono" }, row.loader),
                  formatNumber(row.objects),
                  formatBytes(row.bytes),
                  formatNumber(row.classes),
                ]),
                { dense: true, empty: "No loader information" },
              ).wrapper,
            );
          }
        }
        renderTabs();
      }

      async function openExternal(): Promise<void> {
        try {
          const path = await pickHprofFile();
          if (path) await analyse(path);
        } catch (error) {
          notify.error("Could not open dump", friendlyError(error));
        }
      }

      async function capture(pageContext: PageContext): Promise<void> {
        if (!pageContext.pid) return;
        try {
          const dump = await api.captureHeapDump(pageContext.pid);
          notify.ok("Heap dump captured", dump.path);
          await loadDumps();
          await analyse(dump.path);
        } catch (error) {
          notify.error("Heap dump failed", friendlyError(error));
        }
      }

      async function removeDump(dump: DumpInfo): Promise<void> {
        const deleteFile = checkbox(`Also delete ${dump.path.split(/[\\/]/).pop() ?? "the file"} from disk`);
        const confirmed = await confirmAction({
          title: "Remove dump?",
          message: "The dump record will be removed from JCC. You can optionally delete the file as well.",
          confirmLabel: "Remove",
          danger: true,
          extra: [deleteFile.root],
        });
        if (!confirmed) return;
        try {
          await api.deleteDump(dump.id, deleteFile.checked);
          notify.ok("Dump removed");
          if (current?.path === dump.path) current = null;
          await loadDumps();
        } catch (error) {
          notify.error("Could not remove dump", friendlyError(error));
        }
      }

      void loadDumps();
      return () => {
        disposed = true;
        for (const chart of charts) chart.dispose();
      };
    },
  };
}
