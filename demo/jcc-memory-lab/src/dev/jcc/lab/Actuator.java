package dev.jcc.lab;

import com.sun.net.httpserver.HttpExchange;
import com.sun.net.httpserver.HttpServer;

import java.io.IOException;
import java.io.OutputStream;
import java.net.InetSocketAddress;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.Executors;

/** Minimal Spring-Actuator-compatible endpoint so JCC's Spring page can probe it. */
@Singleton
public final class Actuator {
    @Inject private LabConfig config;
    @Inject private Allocator allocator;
    @Inject private MemoryStore store;

    private HttpServer server;

    public void start() {
        try {
            server = HttpServer.create(new InetSocketAddress(config.port()), 0);
            server.createContext("/actuator", this::route);
            server.setExecutor(Executors.newFixedThreadPool(2));
            server.start();
            System.out.printf("Actuator ready at http://localhost:%d/actuator%n", config.port());
        } catch (IOException error) {
            System.err.println("Actuator port unavailable: " + error.getMessage());
        }
    }

    public void stop() {
        if (server != null) server.stop(0);
    }

    private void route(HttpExchange exchange) throws IOException {
        String path = exchange.getRequestURI().getPath();
        String body;
        switch (path) {
            case "/actuator", "/actuator/":
                body = """
                    {"_links":{"health":{"href":"/actuator/health"},"info":{"href":"/actuator/info"},
                    "metrics":{"href":"/actuator/metrics"},"jcc-lab":{"href":"/actuator/jcc/lab"}}}""";
                break;
            case "/actuator/health":
                Runtime runtime = Runtime.getRuntime();
                body = String.format("""
                    {"status":"UP","components":{"memory":{"status":"UP","details":{"heapUsed":%d,
                    "heapMax":%d,"expectedRetained":%d,"retainedBlocks":%d}}}}""",
                    runtime.totalMemory() - runtime.freeMemory(), runtime.maxMemory(),
                    config.expectedRetainedBytes(), store.retainedCount());
                break;
            case "/actuator/info":
                body = String.format("""
                    {"app":{"name":"jcc-memory-lab","version":"0.1.0"},"di":{"container":"dev.jcc.lab.Container",
                    "beans":8},"workload":{"blockKib":%d,"retainedBlocks":%d,"churnBlocksPerTick":%d,
                    "expectedRetainedBytes":%d,"expectedChurnBytesPerSecond":%d}}""",
                    config.blockKib(), config.retainedBlocks(), config.churnBlocksPerTick(),
                    config.expectedRetainedBytes(), config.expectedChurnPerSecondBytes());
                break;
            case "/actuator/metrics":
                body = """
                    {"names":["jvm.memory.used","jvm.memory.max","jvm.threads.live","process.cpu.usage",
                    "jcc.lab.retained.bytes","jcc.lab.allocated.total.bytes","jcc.lab.blocks.retained",
                    "jcc.lab.churn.cycles","jcc.lab.expected.retained.bytes"]}""";
                break;
            case "/actuator/metrics/jvm.memory.used":
                body = metric("jvm.memory.used", "heap memory currently used", "bytes",
                    Runtime.getRuntime().totalMemory() - Runtime.getRuntime().freeMemory());
                break;
            case "/actuator/metrics/jvm.memory.max":
                body = metric("jvm.memory.max", "maximum heap", "bytes", Runtime.getRuntime().maxMemory());
                break;
            case "/actuator/metrics/jvm.threads.live":
                body = metric("jvm.threads.live", "live threads", null, Thread.activeCount());
                break;
            case "/actuator/metrics/process.cpu.usage":
                body = metric("process.cpu.usage", "recent CPU usage", null, 0.0);
                break;
            case "/actuator/metrics/jcc.lab.retained.bytes":
                body = metric("jcc.lab.retained.bytes", "strongly retained payload", "bytes", store.retainedBytes());
                break;
            case "/actuator/metrics/jcc.lab.allocated.total.bytes":
                body = metric("jcc.lab.allocated.total.bytes", "all bytes allocated since start", "bytes", store.allocatedBytes());
                break;
            case "/actuator/metrics/jcc.lab.blocks.retained":
                body = metric("jcc.lab.blocks.retained", "number of retained blocks", null, store.retainedCount());
                break;
            case "/actuator/metrics/jcc.lab.churn.cycles":
                body = metric("jcc.lab.churn.cycles", "churn ticks completed", null, allocator.churnCycles());
                break;
            case "/actuator/metrics/jcc.lab.expected.retained.bytes":
                body = metric("jcc.lab.expected.retained.bytes", "predicted retained payload", "bytes", config.expectedRetainedBytes());
                break;
            case "/actuator/jcc/lab":
                body = String.format("""
                    {"blockBytes":%d,"retainedBlocks":%d,"retainedBytes":%d,"expectedRetainedBytes":%d,
                    "allocatedTotalBytes":%d,"transientBytes":%d,"churnCycles":%d,"churnPerSecondBytes":%d,
                    "heapUsed":%d,"heapMax":%d}""",
                    config.blockBytes(), store.retainedCount(), store.retainedBytes(), config.expectedRetainedBytes(),
                    store.allocatedBytes(), store.transientBytes(), allocator.churnCycles(),
                    config.expectedChurnPerSecondBytes(),
                    Runtime.getRuntime().totalMemory() - Runtime.getRuntime().freeMemory(),
                    Runtime.getRuntime().maxMemory());
                break;
            default:
                send(exchange, 404, "{\"timestamp\":\"now\",\"status\":404,\"error\":\"Not Found\"}");
                return;
        }
        send(exchange, 200, body);
    }

    private static String metric(String name, String description, String baseUnit, Number value) {
        String unit = baseUnit == null ? "null" : "\"" + baseUnit + "\"";
        return String.format("""
            {"name":"%s","description":"%s","baseUnit":%s,
            "measurements":[{"statistic":"VALUE","value":%s}]}""", name, description, unit, value);
    }

    private static void send(HttpExchange exchange, int status, String json) throws IOException {
        byte[] bytes = json.getBytes(StandardCharsets.UTF_8);
        exchange.getResponseHeaders().add("Content-Type", "application/json");
        exchange.sendResponseHeaders(status, bytes.length);
        try (OutputStream out = exchange.getResponseBody()) {
            out.write(bytes);
        }
    }

}
