package dev.jcc.lab;

/** Immutable workload configuration parsed from command-line arguments. */
@Singleton
public final class LabConfig {
    private int blockKib = 256;
    private int retainedBlocks = 200;
    private int churnBlocksPerTick = 4;
    private int churnIntervalMs = 500;
    private int workerThreads = 2;
    private int port = 8081;
    private long durationSecs = 0;

    public void parse(String[] args) {
        for (int i = 0; i < args.length; i++) {
            String arg = args[i];
            String next = i + 1 < args.length ? args[i + 1] : null;
            switch (arg) {
                case "--block-kib" -> blockKib = valueInt(arg, next);
                case "--retained-blocks" -> retainedBlocks = valueInt(arg, next);
                case "--churn-blocks" -> churnBlocksPerTick = valueInt(arg, next);
                case "--churn-interval-ms" -> churnIntervalMs = valueInt(arg, next);
                case "--threads" -> workerThreads = valueInt(arg, next);
                case "--port" -> port = valueInt(arg, next);
                case "--duration-secs" -> durationSecs = Long.parseLong(require(arg, next));
                case "--help" -> {
                    printHelp();
                    System.exit(0);
                }
                default -> System.err.printf("Unknown argument '%s' (ignored)%n", arg);
            }
            if (arg.startsWith("--") && !arg.equals("--help")) i++;
        }
        if (blockKib < 1 || retainedBlocks < 0 || churnBlocksPerTick < 0 || workerThreads < 1) {
            throw new IllegalArgumentException("block-kib, retained-blocks and threads must be positive");
        }
    }

    private static String require(String arg, String next) {
        if (next == null) throw new IllegalArgumentException("Missing value for " + arg);
        return next;
    }

    private static int valueInt(String arg, String next) {
        return Integer.parseInt(require(arg, next));
    }

    private static void printHelp() {
        System.out.println("""
            Java Control Centre — Memory Lab
              --block-kib N          size of one byte[] block (default 256)
              --retained-blocks N    blocks kept alive (default 200 -> 50 MiB)
              --churn-blocks N       temporary blocks per tick (default 4 -> 1 MiB/tick)
              --churn-interval-ms N  churn tick interval (default 500)
              --threads N            named worker threads (default 2)
              --port N               Actuator port (default 8081)
              --duration-secs N      auto-exit after N seconds (default 0 = forever)
            """);
    }

    public int blockKib() { return blockKib; }
    public int blockBytes() { return blockKib * 1024; }
    public int retainedBlocks() { return retainedBlocks; }
    public int churnBlocksPerTick() { return churnBlocksPerTick; }
    public int churnIntervalMs() { return churnIntervalMs; }
    public int workerThreads() { return workerThreads; }
    public int port() { return port; }
    public long durationSecs() { return durationSecs; }

    /** Retained payload plus a conservative per-array object header estimate. */
    public long expectedRetainedBytes() {
        return (long) retainedBlocks * (blockBytes() + 32);
    }

    public long expectedChurnPerSecondBytes() {
        return (long) churnBlocksPerTick * blockBytes() * (1000L / Math.max(1, churnIntervalMs));
    }
}
