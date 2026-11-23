package dev.jcc.lab;

import java.util.concurrent.atomic.AtomicLong;

/** Injected service that performs the configured retained and churn allocations. */
@Singleton
public final class Allocator {
    @Inject private LabConfig config;
    @Inject private MemoryStore store;
    @Inject(SystemClock.class) private Clock clock;

    private final AtomicLong churnCycles = new AtomicLong();
    private volatile long startedAt;

    public void initialise() {
        startedAt = clock.nowMillis();
        long max = Runtime.getRuntime().maxMemory();
        if (config.expectedRetainedBytes() > max * 0.8) {
            System.err.printf("WARNING: retained payload (%d MiB) exceeds 80%% of -Xmx (%d MiB)%n",
                config.expectedRetainedBytes() >> 20, max >> 20);
        }
        for (int i = 0; i < config.retainedBlocks(); i++) store.retain();
    }

    /** Allocates temporary garbage; the returned array is deliberately dropped. */
    public void churnOnce() {
        for (int i = 0; i < config.churnBlocksPerTick(); i++) {
            byte[] block = store.churn();
            if (block[0] < 0) System.err.println("Unexpected byte value");
        }
        churnCycles.incrementAndGet();
    }

    public long churnCycles() { return churnCycles.get(); }
    public long uptimeMillis() { return clock.nowMillis() - startedAt; }
    public MemoryStore store() { return store; }
}
