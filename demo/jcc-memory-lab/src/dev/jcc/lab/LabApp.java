package dev.jcc.lab;

/**
 * A memory test application for Java Control Centre.
 *
 * Recommended launch:
 *   java -Xms256m -Xmx512m -XX:+UseG1GC \
 *        -Dserver.port=8081 -cp out dev.jcc.lab.LabApp \
 *        --block-kib 256 --retained-blocks 200 --churn-blocks 4 --threads 2
 *
 * Then select "LabApp" in JCC and open:
 *   Memory Explorer  -> heap used ~= retained payload + JVM overhead
 *   Class Loading    -> dev.jcc.lab.* beans loaded by the DI container
 *   Thread Explorer  -> jcc-lab-churn-N and jcc-lab-report
 *   Spring Monitor   -> http://localhost:8081/actuator (custom jcc.lab.* metrics)
 */
public final class LabApp {
    public static void main(String[] args) throws Exception {
        LabConfig config = new LabConfig();
        config.parse(args);

        Container container = new Container(
            LabConfig.class,
            SystemClock.class,
            MemoryStore.class,
            Allocator.class,
            GcPressure.class,
            Actuator.class,
            Reporter.class
        );
        container.registerInstance(LabConfig.class, config);
        container.init();

        Allocator allocator = container.get(Allocator.class);
        GcPressure pressure = container.get(GcPressure.class);
        Actuator actuator = container.get(Actuator.class);
        Reporter reporter = container.get(Reporter.class);

        allocator.initialise();
        reporter.banner();
        pressure.start();
        actuator.start();
        reporter.start();

        Runtime.getRuntime().addShutdownHook(new Thread(() -> {
            pressure.stop();
            reporter.stop();
            actuator.stop();
        }, "jcc-lab-shutdown"));

        if (config.durationSecs() > 0) {
            Thread.sleep(config.durationSecs() * 1000);
            pressure.stop();
            reporter.stop();
            actuator.stop();
            System.out.println("Memory Lab finished.");
        }
    }
}
