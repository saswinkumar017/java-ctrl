import { api } from "../lib/api";
import { ChartHost, horizontalBarOption, palette } from "../lib/charts";
import { clear, h } from "../lib/dom";
import { formatBytes, formatMs, formatNumber } from "../lib/format";
import { badge, createCard, progressBar, refreshButton, searchInput, statTile, table } from "../lib/ui";
import { infoNotice, noProcess, pageHead } from "./common";
import type { Cleanup, PageDefinition } from "./page";
import type { JitSnapshot } from "../types";

export function jitPage(): PageDefinition {
  return {
    path: "/jit",
    title: "JIT Explorer",
    description: "Compiler activity and code cache",
    group: "Observe",
    icon: "zap",
    mount(root, context): Cleanup {
      root.appendChild(
        pageHead(
          "JIT Explorer",
          "Reads Compiler.codelist, Compiler.queue, CodeCache and compiler counters with jcmd.",
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

      const grid = h("div", { className: "grid-2" });
      const heapsCard = createCard({ title: "Code heaps", subtitle: "Used capacity per code heap", iconName: "memory" });
      const cacheCard = createCard({ title: "Code cache", subtitle: "Aggregate capacity and compiler threads", iconName: "server" });
      const heapEl = h("div", { className: "chart chart-md" });
      heapsCard.body.appendChild(heapEl);
      grid.appendChild(heapsCard.root);
      grid.appendChild(cacheCard.root);
      content.appendChild(grid);
      const chart = new ChartHost(heapEl);

      const methodsCard = createCard({ title: "Compiled methods", subtitle: "From Compiler.codelist", iconName: "list" });
      content.appendChild(methodsCard.root);
      const search = searchInput("Filter methods…", (value) => {
        filter = value;
        renderMethods();
      });
      methodsCard.header?.appendChild(search.wrapper);

      const queueCard = createCard({ title: "Compiler queue", subtitle: "Methods waiting for compilation", iconName: "clock" });
      content.appendChild(queueCard.root);

      let disposed = false;
      let snapshot: JitSnapshot | null = null;
      let filter = "";

      async function load(): Promise<void> {
        try {
          const value = await api.getJit(pid);
          if (disposed) return;
          snapshot = value;
          render(value);
        } catch (error) {
          if (disposed) return;
          clear(stats);
          stats.appendChild(
            h("div", { className: "notice notice-error" }, `Could not read JIT data: ${error instanceof Error ? error.message : String(error)}`),
          );
        }
      }

      function render(value: JitSnapshot): void {
        clear(noticeHost);
        if (value.notes.length) noticeHost.appendChild(infoNotice(value.notes.join(" ")));
        renderStats(value);
        chart.setOption(
          horizontalBarOption(
            value.codeHeaps.map((heap) => ({ name: heap.name, value: heap.used, color: palette.amber })),
            { valueFormat: formatBytes },
          ),
        );
        renderCache(value);
        renderMethods();
        renderQueue(value);
      }

      function renderStats(value: JitSnapshot): void {
        clear(stats);
        stats.appendChild(statTile({ label: "Compilations", value: formatNumber(value.compiles), sub: "total since startup", iconName: "zap", tone: "accent" }));
        stats.appendChild(statTile({ label: "Compile time", value: formatMs(value.timeMs), sub: "cumulative", iconName: "clock", tone: "info" }));
        stats.appendChild(statTile({ label: "Installed methods", value: formatNumber(value.nmethods), sub: `${formatNumber(value.totalCompiledMethods)} in codelist`, iconName: "list", tone: "ok" }));
        stats.appendChild(statTile({ label: "Nmethod size", value: formatBytes(value.nmethodCodeSize), sub: "native machine code", iconName: "memory" }));
        stats.appendChild(statTile({ label: "Bailouts", value: formatNumber(value.bailouts), sub: "compilations aborted", iconName: "alert", tone: value.bailouts ? "warn" : "neutral" }));
        stats.appendChild(statTile({ label: "Invalidations", value: formatNumber(value.invalidates), sub: "methods deoptimised", iconName: "recycle", tone: value.invalidates ? "warn" : "neutral" }));
      }

      function renderCache(value: JitSnapshot): void {
        clear(cacheCard.body);
        cacheCard.body.appendChild(
          h(
            "div",
            {},
            h(
              "div",
              { className: "flex-row", style: { justifyContent: "space-between", marginBottom: "4px" } },
              h("span", {}, "Total code cache"),
              h("span", { className: "mono dim" }, `${formatBytes(value.codeUsed)} / ${formatBytes(value.codeTotal)}`),
            ),
            progressBar(value.codeUsed, value.codeTotal),
            h("div", { className: "cell-sub", style: { marginBottom: "14px" } }, `peak ${formatBytes(value.codeMaxUsed)} · free ${formatBytes(value.codeFree)}`),
            ...value.codeHeaps.map((heap) =>
              h(
                "div",
                { style: { marginBottom: "9px" } },
                h(
                  "div",
                  { className: "flex-row", style: { justifyContent: "space-between" } },
                  h("span", {}, heap.name),
                  h("span", { className: "mono dim" }, `${formatBytes(heap.used)} / ${formatBytes(heap.size)}`),
                ),
                progressBar(heap.used, heap.size, "info"),
              ),
            ),
            h("div", { className: "flex-row", style: { marginTop: "12px" } },
              badge(value.compilationEnabled ? "Compilation enabled" : "Compilation disabled", value.compilationEnabled ? "ok" : "danger"),
              h("span", { className: "muted" }, `${value.compilerThreads} compiler threads`),
            ),
          ),
        );
      }

      function renderMethods(): void {
        if (!snapshot) return;
        clear(methodsCard.body);
        const methods = snapshot.compiledMethods.filter((method) => !filter || method.method.toLowerCase().includes(filter));
        methodsCard.body.appendChild(
          table(
            [
              { label: "ID", width: "80px" },
              { label: "Level", width: "70px" },
              { label: "Sync", align: "right", width: "60px" },
              { label: "Method" },
              { label: "Address range" },
            ],
            methods.slice(0, 300).map((method) => [
              h("span", { className: "mono" }, String(method.id)),
              badge(`L${method.level}`, method.level >= 4 ? "accent" : "neutral"),
              String(method.sync),
              h("span", { className: "mono ellipsis", title: method.method }, method.method),
              h("span", { className: "mono dim" }, method.addressRange),
            ]),
            { dense: true, empty: "No compiled methods in the codelist" },
          ).wrapper,
        );
      }

      function renderQueue(value: JitSnapshot): void {
        clear(queueCard.body);
        if (value.queue.length === 0) {
          queueCard.body.appendChild(h("p", { className: "muted" }, "Compiler queue is empty."));
          return;
        }
        queueCard.body.appendChild(
          h("ol", { className: "stack-list", style: { maxHeight: "220px", overflow: "auto" } },
            ...value.queue.map((item) => h("li", { style: { color: "var(--text-2)" } }, item))),
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
