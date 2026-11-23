package dev.jcc.lab;

import java.util.concurrent.Executors;
import java.util.concurrent.ScheduledExecutorService;
import java.util.concurrent.TimeUnit;

/** Prints the expected-vs-actual heap accounting every two seconds. */
@Singleton
public final class Reporter {
    @Inject private LabConfig config;
    @Inject private Allocator allocator;
    @Inject private MemoryStore store;

    private ScheduledExecutorService reporter;

    public void banner() {
        long expected = config.expectedRetainedBytes();
        System.out.printf("""
            ------------------------------------------------------------------
             JCC Memory Lab — deterministic heap workload
             block size            : %d KiB
             retained blocks       : %d  -> ~%d MiB payload (+ ~%d KiB headers)
             churn                 : %d blocks/tick every %d ms -> ~%d MiB/s garbage
             named worker threads  : jcc-lab-churn-N
             DI container          : dev.jcc.lab.Container (8 singleton beans)
             expected live heap    : ~%d MiB + 15-25 MiB JVM/DI/JIT overhead
            ------------------------------------------------------------------
            """, config.blockKib(), config.retainedBlocks(), expected >> 20,
            (config.retainedBlocks() * 32L) >> 10, config.churnBlocksPerTick(),
            config.churnIntervalMs(), config.expectedChurnPerSecondBytes() >> 20, expected >> 20);
    }

    public void start() {
        reporter = Executors.newSingleThreadScheduledExecutor(runnable -> {
            Thread thread = new Thread(runnable, "jcc-lab-report");
            thread.setDaemon(false);
            return thread;
        });
        reporter.scheduleAtFixedRate(this::report, 0, 2, TimeUnit.SECONDS);
    }

    private void report() {
        Runtime runtime = Runtime.getRuntime();
        long used = runtime.totalMemory() - runtime.freeMemory();
        System.out.printf(
            "heap used=%d MiB / max=%d MiB | retained=%d MiB (%d blocks) | allocated total=%d MiB | churn=%d MiB | cycles=%d%n",
            used >> 20, runtime.maxMemory() >> 20, store.retainedBytes() >> 20,
            store.retainedCount(), store.allocatedBytes() >> 20, store.transientBytes() >> 20,
            allocator.churnCycles());
    }

    public void stop() {
        if (reporter != null) reporter.shutdownNow();
    }
}
