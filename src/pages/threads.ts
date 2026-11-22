import { api, copyToClipboard } from "../lib/api";
import { ChartHost, donutOption, palette } from "../lib/charts";
import { clear, h } from "../lib/dom";
import { formatMs, formatNumber } from "../lib/format";
import { friendlyError, notify } from "../lib/toast";
import {
  badge,
  button,
  createCard,
  refreshButton,
  searchInput,
  spinner,
  statTile,
  table,
} from "../lib/ui";
import { errorNotice, noProcess, pageHead, warnNotice } from "./common";
import type { Cleanup, PageDefinition } from "./page";
import type { ThreadSnapshot } from "../types";

const STATE_COLORS = [palette.cyan, palette.amber, palette.red, palette.violet, palette.slate, palette.green];

export function threadsPage(): PageDefinition {
  return {
    path: "/threads",
    title: "Thread Explorer",
    description: "Thread.print stacks, states and deadlock detection",
    group: "Observe",
    icon: "layers",
    mount(root, context): Cleanup {
      root.appendChild(
        pageHead(
          "Thread Explorer",
          "Full thread dumps are read with jcmd Thread.print and parsed in-process.",
          [
            refreshButton(() => void load()),
            button({ label: "Copy dump", iconName: "copy", variant: "subtle", onClick: () => void copyDump() }),
          ],
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
      stats.appendChild(spinner("Reading thread dump…"));

      const grid = h("div", { className: "grid-2" });
      const distributionCard = createCard({ title: "State distribution", subtitle: "Threads by reported state", iconName: "activity" });
      const topCard = createCard({ title: "Busiest threads", subtitle: "Highest CPU time in the dump", iconName: "zap" });
      grid.appendChild(distributionCard.root);
      grid.appendChild(topCard.root);
      content.appendChild(grid);

      const listCard = createCard({ title: "Threads", subtitle: "Click a row to expand its stack trace", iconName: "list" });
      content.appendChild(listCard.root);

      const search = searchInput("Filter by name, state, tid or stack frame…", (value) => {
        filter = value;
        renderTable();
      });
      const stateFilter = h("select", { className: "input select" }, h("option", { value: "" }, "All states"));
      stateFilter.addEventListener("change", () => {
        state = stateFilter.value;
        renderTable();
      });
      listCard.header?.appendChild(
        h("div", { className: "flex-row" }, search.wrapper, stateFilter),
      );

      let disposed = false;
      let snapshot: ThreadSnapshot | null = null;
      let filter = "";
      let state = "";
      let chart: ChartHost | null = null;
      let expanded = new Set<string>();

      async function load(): Promise<void> {
        try {
          const value = await api.getThreads(pid);
          if (disposed) return;
          snapshot = value;
          render(value);
        } catch (error) {
          if (disposed) return;
          clear(stats);
          stats.appendChild(errorNotice(error, "Thread dump failed"));
        }
      }

      function render(value: ThreadSnapshot): void {
        clear(noticeHost);
        if (value.deadlock) noticeHost.appendChild(warnNotice("Deadlock detected: " + (value.deadlockText ?? "see the dump below")));
        renderStats(value);
        renderDistribution(value);
        renderTop(value);
        clear(stateFilter);
        stateFilter.appendChild(h("option", { value: "" }, "All states"));
        const states = new Set(value.threads.map((thread) => thread.state));
        for (const name of states) stateFilter.appendChild(h("option", { value: name, selected: name === state }, name));
        renderTable();
      }

      function renderStats(value: ThreadSnapshot): void {
        clear(stats);
        stats.appendChild(statTile({ label: "Total threads", value: formatNumber(value.total), sub: `${value.threads.length} parsed`, iconName: "layers" }));
        stats.appendChild(statTile({
          label: "Daemon",
          value: formatNumber(value.threads.filter((thread) => thread.daemon).length),
          sub: "background threads",
          iconName: "shield",
          tone: "info",
        }));
        stats.appendChild(statTile({
          label: "Blocked / waiting",
          value: formatNumber(Object.entries(value.counts).filter(([key]) => /block|wait|park|sleep/i.test(key)).reduce((sum, [, count]) => sum + count, 0)),
          sub: "not runnable",
          iconName: "clock",
          tone: "warn",
        }));
        stats.appendChild(statTile({
          label: "Deadlocks",
          value: value.deadlock ? "YES" : "None",
          sub: "JVM deadlock detector",
          iconName: "alert",
          tone: value.deadlock ? "danger" : "ok",
        }));
      }

      function renderDistribution(value: ThreadSnapshot): void {
        clear(distributionCard.body);
        const entries = Object.entries(value.counts).sort((a, b) => b[1] - a[1]);
        if (chart) {
          chart.dispose();
          chart = null;
        }
        const element = h("div", { className: "chart chart-md" });
        distributionCard.body.appendChild(element);
        chart = new ChartHost(element);
        chart.setOption(donutOption(entries.map(([name, count]) => ({ name, value: count })), STATE_COLORS));
      }

      function renderTop(value: ThreadSnapshot): void {
        clear(topCard.body);
        const busiest = [...value.threads].sort((a, b) => b.cpuMs - a.cpuMs).slice(0, 8);
        topCard.body.appendChild(
          table(
            [
              { label: "Thread" },
              { label: "State" },
              { label: "CPU", align: "right" },
              { label: "Elapsed", align: "right" },
            ],
            busiest.map((thread) => [
              h("span", { className: "mono ellipsis", title: thread.name }, thread.name),
              stateBadge(thread.state),
              formatMs(thread.cpuMs),
              formatMs(thread.elapsedSecs * 1000),
            ]),
            { dense: true, empty: "No thread CPU data available" },
          ).wrapper,
        );
      }

      function renderTable(): void {
        if (!snapshot) return;
        clear(listCard.body);
        const threads = snapshot.threads.filter((thread) => {
          if (state && thread.state !== state) return false;
          if (!filter) return true;
          return [thread.name, thread.state, thread.tid ?? "", thread.stack.join(" "), thread.locks.join(" ")]
            .join(" ").toLowerCase().includes(filter);
        });
        const rows: HTMLTableRowElement[] = [];
        threads.forEach((thread) => {
          const row = h(
            "tr",
            {
              className: ["thread-row", expanded.has(thread.name) ? "row-selected" : ""].filter(Boolean),
              onClick: () => {
                if (expanded.has(thread.name)) expanded.delete(thread.name);
                else expanded.add(thread.name);
                renderTable();
              },
            },
            h("td", {}, h("div", { className: "cell-stack" }, h("span", { className: "mono" }, thread.name), h("span", { className: "cell-sub" }, thread.header.slice(0, 110)))),
            h("td", {}, stateBadge(thread.state)),
            h("td", {}, thread.daemon ? badge("daemon", "neutral") : h("span", { className: "dim" }, "user")),
            h("td", { className: "num" }, thread.priority == null ? "—" : String(thread.priority)),
            h("td", { className: "num" }, formatMs(thread.cpuMs)),
            h("td", { className: "num" }, formatMs(thread.elapsedSecs * 1000)),
            h("td", { className: "mono dim" }, thread.tid ?? "—"),
          );
          rows.push(row);
        });
        const view = table(
          [
            { label: "Thread name" },
            { label: "State", width: "110px" },
            { label: "Kind", width: "90px" },
            { label: "Prio", align: "right", width: "60px" },
            { label: "CPU time", align: "right", width: "100px" },
            { label: "Elapsed", align: "right", width: "100px" },
            { label: "TID", width: "110px" },
          ],
          rows,
          { dense: true, empty: "No threads match the filter" },
        );
        listCard.body.appendChild(view.wrapper);
        for (const [index, thread] of threads.entries()) {
          if (!expanded.has(thread.name)) continue;
          const detail = h(
            "div",
            { className: "thread-detail" },
            h("div", { className: "flex-row" }, h("strong", {}, thread.name), badge(thread.state, "info"), thread.daemon ? badge("daemon", "neutral") : null),
            h("p", { className: "mono dim", style: { fontSize: "10px" } }, thread.header),
            h("div", { className: "flex-row", style: { gap: "18px", fontSize: "11px" } },
              h("span", {}, `OS priority: ${thread.osPriority ?? "—"}`),
              h("span", {}, `NID: ${thread.nid ?? "—"}`),
              h("span", {}, `CPU: ${formatMs(thread.cpuMs)}`),
              h("span", {}, `Elapsed: ${formatMs(thread.elapsedSecs * 1000)}`),
            ),
            h("h4", { style: { margin: "12px 0 4px", fontSize: "11px", color: "var(--muted)" } }, "Stack"),
            h("ol", { className: "stack-list" }, ...thread.stack.map((frame) => h("li", {}, frame))),
            thread.locks.length
              ? h("div", {}, h("h4", { style: { margin: "10px 0 4px", fontSize: "11px", color: "var(--muted)" } }, "Locks"), ...thread.locks.map((lock) => h("div", { className: "mono dim", style: { fontSize: "10px" } }, lock)))
              : null,
          );
          const target = rows[index];
          if (target) target.after(detail);
        }
      }

      function stateBadge(state: string): HTMLElement {
        const tone = /block/i.test(state) ? "danger" : /wait|park|sleep|idle/i.test(state) ? "warn" : /runnable/i.test(state) ? "ok" : "neutral";
        return h("span", { className: "thread-state" }, h("span", { className: ["thread-state-dot", `thread-state-${tone}`].filter(Boolean) }), state);
      }

      async function copyDump(): Promise<void> {
        try {
          const latest = snapshot ?? (await api.getThreads(pid));
          await copyToClipboard(latest.raw);
          notify.ok("Thread dump copied to clipboard");
        } catch (error) {
          notify.error("Could not copy dump", friendlyError(error));
        }
      }

      void load();
      const timer = window.setInterval(() => {
        if (!document.hidden) void load();
      }, 15_000);

      return () => {
        disposed = true;
        window.clearInterval(timer);
        chart?.dispose();
      };
    },
  };
}

