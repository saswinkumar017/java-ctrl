import com.sun.net.httpserver.HttpExchange;
import com.sun.net.httpserver.HttpServer;

import java.io.IOException;
import java.io.OutputStream;
import java.net.InetSocketAddress;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.Random;
import java.util.concurrent.Executors;
import java.util.concurrent.ScheduledExecutorService;
import java.util.concurrent.TimeUnit;

/**
 * A dependency-free demo JVM for exercising Java Control Centre.
 *
 *   javac demo/DemoApp.java -d demo
 *   java -Dserver.port=8080 -Xmx256m -cp demo DemoApp
 *
 * It allocates to trigger GCs, runs a few named threads and serves a tiny
 * Spring-Boot-compatible Actuator endpoint on http://localhost:8080/actuator
 * so the Spring Boot Monitor page has something to probe.
 */
public class DemoApp {
    private static final List<byte[]> LIVE = new ArrayList<>();
    private static final Random RANDOM = new Random();

    public static void main(String[] args) throws Exception {
        int port = args.length > 0 ? Integer.parseInt(args[0]) : 8080;
        HttpServer server = HttpServer.create(new InetSocketAddress(port), 0);
        server.createContext("/actuator", DemoApp::handleActuator);
        server.setExecutor(Executors.newFixedThreadPool(2));
        server.start();

        ScheduledExecutorService workers = Executors.newScheduledThreadPool(3, runnable -> {
            Thread thread = new Thread(runnable);
            thread.setDaemon(false);
            return thread;
        });
        workers.scheduleAtFixedRate(DemoApp::allocate, 0, 250, TimeUnit.MILLISECONDS);
        workers.scheduleAtFixedRate(DemoApp::logStatus, 0, 2, TimeUnit.SECONDS);

        System.out.println("Demo JVM ready. Actuator on http://localhost:" + port + "/actuator");
    }

    private static void allocate() {
        LIVE.add(new byte[64 * 1024]);
        if (LIVE.size() > 1500) LIVE.clear();
        byte[] throwaway = new byte[RANDOM.nextInt(256 * 1024) + 64 * 1024];
        throwaway[0] = 1;
    }

    private static void logStatus() {
        Runtime runtime = Runtime.getRuntime();
        System.out.printf("heap used=%d/%d MiB, threads=%d%n",
                (runtime.totalMemory() - runtime.freeMemory()) >> 20,
                runtime.maxMemory() >> 20,
                Thread.activeCount());
    }

    private static void handleActuator(HttpExchange exchange) throws IOException {
        String path = exchange.getRequestURI().getPath();
        String body;
        switch (path) {
            case "/actuator", "/actuator/":
                body = "{\"_links\":{\"health\":{\"href\":\"/actuator/health\"},"
                        + "\"info\":{\"href\":\"/actuator/info\"},"
                        + "\"metrics\":{\"href\":\"/actuator/metrics\"}}}";
                break;
            case "/actuator/health":
                body = "{\"status\":\"UP\",\"components\":{\"diskSpace\":{\"status\":\"UP\"}}}";
                break;
            case "/actuator/info":
                body = "{\"app\":{\"name\":\"jcc-demo\",\"version\":\"1.0.0\"}}";
                break;
            case "/actuator/metrics":
                body = "{\"names\":[\"jvm.memory.used\",\"jvm.threads.live\",\"process.cpu.usage\"]}";
                break;
            case "/actuator/metrics/jvm.memory.used":
                Runtime runtime = Runtime.getRuntime();
                long used = runtime.totalMemory() - runtime.freeMemory();
                body = "{\"name\":\"jvm.memory.used\",\"description\":\"Used heap memory\","
                        + "\"baseUnit\":\"bytes\",\"measurements\":[{\"statistic\":\"VALUE\",\"value\":" + used + "}]}";
                break;
            default:
                byte[] missing = "{\"timestamp\":\"now\",\"status\":404,\"error\":\"Not Found\"}"
                        .getBytes(StandardCharsets.UTF_8);
                exchange.getResponseHeaders().add("Content-Type", "application/json");
                exchange.sendResponseHeaders(404, missing.length);
                try (OutputStream out = exchange.getResponseBody()) {
                    out.write(missing);
                }
                return;
        }
        byte[] bytes = body.getBytes(StandardCharsets.UTF_8);
        exchange.getResponseHeaders().add("Content-Type", "application/json");
        exchange.sendResponseHeaders(200, bytes.length);
        try (OutputStream out = exchange.getResponseBody()) {
            out.write(bytes);
        }
    }
}
