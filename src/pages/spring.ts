import { api } from "../lib/api";
import { clear, h } from "../lib/dom";
import { formatDateTime, formatNumber, metricValue } from "../lib/format";
import { friendlyError, notify } from "../lib/toast";
import {
  badge,
  button,
  createCard,
  emptyState,
  field,
  iconButton,
  refreshButton,
  spinner,
  table,
  textInput,
} from "../lib/ui";
import { infoNotice, pageHead, warnNotice } from "./common";
import type { Cleanup, PageDefinition } from "./page";
import type { SpringCandidate, SpringConnection, SpringSnapshot } from "../types";

export function springPage(): PageDefinition {
  return {
    path: "/spring",
    title: "Spring Boot Monitor",
    description: "Actuator health, info and metrics",
    group: "Inspect",
    icon: "leaf",
    mount(root): Cleanup {
      root.appendChild(
        pageHead(
          "Spring Boot Monitor",
          "Probes /actuator over HTTP. Discovers candidates from detected JVM command lines and ports.",
          [
            refreshButton(() => void loadConnections()),
            button({ label: "Discover", iconName: "search", variant: "primary", onClick: () => void discover() }),
          ],
        ),
      );

      const layout = h("div", { className: "split-layout" });
      root.appendChild(layout);
      const sidebar = h("div", { className: "stack" });
      layout.appendChild(sidebar);
      const results = h("div", { className: "stack" });
      layout.appendChild(results);

      const candidateCard = createCard({ title: "Detected applications", subtitle: "From JVM arguments and Spring markers", iconName: "search" });
      sidebar.appendChild(candidateCard.root);
      candidateCard.body.appendChild(h("p", { className: "muted" }, "Run discovery to see Spring Boot candidates."));

      const formCard = createCard({ title: "Connect", subtitle: "Actuator endpoint with optional basic auth", iconName: "leaf" });
      sidebar.appendChild(formCard.root);
      const nameInput = textInput("Orders service");
      const urlInput = textInput("http://localhost:8080", "http://localhost:8080");
      const userInput = textInput("username");
      const passwordInput = textInput("password", "", "password");
      formCard.body.appendChild(
        h(
          "div",
          { className: "stack", style: { gap: "12px" } },
          field("Display name (optional)", nameInput),
          field("Base URL", urlInput, "JCC appends /actuator/health, /info and /metrics"),
          h("div", { className: "form-grid" }, field("Username", userInput), field("Password", passwordInput)),
          h("div", { className: "flex-row" },
            button({ label: "Probe now", iconName: "activity", variant: "primary", onClick: () => void probeDirect() }),
            button({ label: "Save connection", iconName: "plus", variant: "secondary", onClick: () => void saveConnection() }),
          ),
        ),
      );

      const savedCard = createCard({ title: "Saved connections", subtitle: "Credentials are stored in the local database", iconName: "database" });
      sidebar.appendChild(savedCard.root);
      savedCard.body.appendChild(spinner("Loading connections…"));

      const resultCard = createCard({
        title: "Actuator snapshot",
        subtitle: "Health, info and key JVM metrics",
        iconName: "activity",
      });
      results.appendChild(resultCard.root);
      resultCard.body.appendChild(
        emptyState({
          iconName: "leaf",
          title: "No probe yet",
          description: "Discover a running Spring Boot application, or enter an Actuator base URL and press Probe now.",
        }),
      );

      let disposed = false;
      let metricFilter = "";

      async function discover(): Promise<void> {
        candidateCard.body.replaceChildren(spinner("Discovering Spring Boot processes…"));
        try {
          const candidates = await api.discoverSpring();
          if (disposed) return;
          renderCandidates(candidates);
          notify.ok("Discovery complete", `${candidates.length} candidate(s) found`);
        } catch (error) {
          if (disposed) return;
          candidateCard.body.replaceChildren(h("p", { className: "notice notice-error" }, friendlyError(error)));
        }
      }

      function renderCandidates(candidates: SpringCandidate[]): void {
        clear(candidateCard.body);
        if (candidates.length === 0) {
          candidateCard.body.appendChild(
            emptyState({ iconName: "search", title: "No candidates found", description: "Make sure the JVM has server.port in its arguments or a spring-boot JAR in its command line." }),
          );
          return;
        }
        for (const candidate of candidates) {
          candidateCard.body.appendChild(
            h(
              "div",
              { className: "dump-row" },
              h("div", { className: "dump-icon" }, "⌁"),
              h(
                "div",
                { className: "dump-main" },
                h("div", {}, `${candidate.name} · PID ${candidate.pid}`),
                h("div", { className: "dump-meta mono" }, candidate.url),
                h("div", { className: "cell-sub" }, `Detected via ${candidate.detectedFrom}`),
                badge(candidate.reachable ? "reachable" : "unreachable", candidate.reachable ? "ok" : "warn"),
              ),
              button({
                label: "Probe",
                iconName: "activity",
                size: "sm",
                variant: "secondary",
                onClick: () => {
                  urlInput.value = candidate.url;
                  void probeDirect();
                },
              }),
            ),
          );
        }
      }

      async function loadConnections(): Promise<void> {
        try {
          const connections = await api.listSpringConnections();
          if (disposed) return;
          renderConnections(connections);
        } catch (error) {
          if (disposed) return;
          savedCard.body.replaceChildren(h("p", { className: "notice notice-error" }, friendlyError(error)));
        }
      }

      function renderConnections(connections: SpringConnection[]): void {
        clear(savedCard.body);
        if (connections.length === 0) {
          savedCard.body.appendChild(h("p", { className: "muted" }, "No saved connections. Probe a URL and save it for later."));
          return;
        }
        savedCard.body.appendChild(
          table(
            [
              { label: "Name" },
              { label: "URL" },
              { label: "Checked" },
              { label: "Actions", align: "right" },
            ],
            connections.map((connection) => [
              h("div", { className: "cell-stack" }, h("span", {}, connection.name), connection.username ? h("span", { className: "cell-sub" }, `user: ${connection.username}`) : null),
              h("span", { className: "mono ellipsis", title: connection.baseUrl }, connection.baseUrl),
              connection.lastCheckedAt ? formatDateTime(connection.lastCheckedAt) : "never",
              h("div", { className: "flex-row", style: { justifyContent: "flex-end", flexWrap: "nowrap" } },
                iconButton("activity", "Probe", () => void probeSaved(connection)),
                iconButton("trash", "Delete", () => void removeConnection(connection)),
              ),
            ]),
            { dense: true },
          ).wrapper,
        );
      }

      async function runProbe(url: string, username?: string, password?: string): Promise<void> {
        resultCard.body.replaceChildren(spinner("Contacting Actuator endpoints…"));
        try {
          const result = await api.probeSpring(url, username, password);
          if (disposed) return;
          renderSnapshot(result);
        } catch (error) {
          if (disposed) return;
          resultCard.body.replaceChildren(
            h("p", { className: "notice notice-error" }, friendlyError(error)),
            infoNotice("Make sure management.endpoints.web.exposure.include=health,info,metrics is configured on the target application."),
          );
        }
      }

      function probeDirect(): void {
        if (!urlInput.value.trim()) {
          notify.warn("Enter a base URL first");
          return;
        }
        void runProbe(urlInput.value, userInput.value || undefined, passwordInput.value || undefined);
      }

      async function probeSaved(connection: SpringConnection): Promise<void> {
        resultCard.body.replaceChildren(spinner("Contacting Actuator endpoints…"));
        try {
          const result = await api.checkSpringConnection(connection.id);
          if (disposed) return;
          urlInput.value = connection.baseUrl;
          renderSnapshot(result);
          await loadConnections();
        } catch (error) {
          if (disposed) return;
          resultCard.body.replaceChildren(h("p", { className: "notice notice-error" }, friendlyError(error)));
        }
      }

      async function saveConnection(): Promise<void> {
        try {
          const connection = await api.saveSpringConnection(
            nameInput.value,
            urlInput.value,
            userInput.value || undefined,
            passwordInput.value || undefined,
          );
          notify.ok("Connection saved", connection.baseUrl);
          await loadConnections();
        } catch (error) {
          notify.error("Could not save connection", friendlyError(error));
        }
      }

      async function removeConnection(connection: SpringConnection): Promise<void> {
        try {
          await api.deleteSpringConnection(connection.id);
          notify.ok("Connection removed");
          await loadConnections();
        } catch (error) {
          notify.error("Could not remove connection", friendlyError(error));
        }
      }

      function renderSnapshot(result: SpringSnapshot): void {
        clear(resultCard.body);
        const status = extractStatus(result.health);
        resultCard.body.appendChild(
          h(
            "div",
            { className: "hero", style: { marginBottom: "16px" } },
            h("div", { className: "hero-glyph" }, "⌁"),
            h(
              "div",
              {},
              h("h2", {}, status),
              h("p", {}, `${result.baseUrl} · probed ${formatDateTime(result.checkedAt)} · ${result.latencyMs} ms`),
              h("div", { className: "hero-tags" },
                badge(`health ${status.toLowerCase()}`, result.health === null ? "warn" : /up/i.test(status) ? "ok" : "danger"),
                badge(`${result.metrics.length} key metrics`, "info"),
              ),
            ),
          ),
        );
        if (result.notes.length) {
          for (const note of result.notes) resultCard.body.appendChild(warnNotice(note));
        }

        const grid = h("div", { className: "grid-2" });
        const infoCard = createCard({ title: "Application info", subtitle: "/actuator/info", iconName: "info" });
        infoCard.body.appendChild(h("pre", { className: "code-block" }, prettyJson(result.info)));
        const linksCard = createCard({ title: "Exposed links", subtitle: "/actuator", iconName: "list" });
        linksCard.body.appendChild(h("pre", { className: "code-block" }, prettyJson(result.links)));
        grid.appendChild(infoCard.root);
        grid.appendChild(linksCard.root);
        resultCard.body.appendChild(grid);

        const metricCard = createCard({ title: "Key metrics", subtitle: "Selected JVM and HTTP metrics", iconName: "activity" });
        const search = h("input", { className: "input input-search", type: "search", placeholder: "Filter metrics…" });
        search.addEventListener("input", () => {
          metricFilter = search.value.trim().toLowerCase();
          renderMetrics();
        });
        metricCard.header?.appendChild(h("div", { className: "search-field" }, search));
        resultCard.body.appendChild(metricCard.root);

        function renderMetrics(): void {
          clear(metricCard.body);
          const metrics = result.metrics.filter((metric) => !metricFilter || metric.name.toLowerCase().includes(metricFilter));
          if (metrics.length === 0) {
            metricCard.body.appendChild(h("p", { className: "muted" }, "No metrics available. Expose metrics in Actuator to see them here."));
            return;
          }
          metricCard.body.appendChild(
            h("div", { className: "grid-3" },
              ...metrics.slice(0, 30).map((metric) => {
                const measurement = metric.measurements[0];
                return h(
                  "div",
                  { className: "metric-card" },
                  h("div", { className: "metric-name", title: metric.description ?? metric.name }, metric.name),
                  h("div", { className: "metric-value" }, measurement ? metricValue(measurement.value, metric.baseUnit) : "—"),
                  h("div", { className: "metric-desc" }, measurement?.statistic ?? metric.baseUnit ?? formatNumber(metric.measurements.length)),
                );
              }),
            ),
          );
        }
        renderMetrics();
      }

      void loadConnections();
      return () => {
        disposed = true;
      };
    },
  };
}

function extractStatus(health: unknown): string {
  if (health && typeof health === "object" && "status" in health) {
    return String((health as Record<string, unknown>).status ?? "unknown");
  }
  return health === null ? "Actuator unavailable" : "UP";
}

function prettyJson(value: unknown): string {
  if (value == null) return "No data returned by Actuator.";
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}
