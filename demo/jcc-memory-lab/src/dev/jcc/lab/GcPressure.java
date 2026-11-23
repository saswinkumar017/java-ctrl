package dev.jcc.lab;

import java.util.concurrent.Executors;
import java.util.concurrent.ScheduledExecutorService;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;

/** Named worker threads that create predictable garbage for the GC Monitor. */
@Singleton
public final class GcPressure {
    @Inject private LabConfig config;
    @Inject private Allocator allocator;

    private final AtomicInteger threadNumber = new AtomicInteger();
    private ScheduledExecutorService workers;

    public void start() {
        workers = Executors.newScheduledThreadPool(config.workerThreads(), runnable -> {
            Thread thread = new Thread(runnable, "jcc-lab-churn-" + threadNumber.incrementAndGet());
            thread.setDaemon(false);
            return thread;
        });
        for (int i = 0; i < config.workerThreads(); i++) {
            workers.scheduleAtFixedRate(this::safeChurn, 0, config.churnIntervalMs(), TimeUnit.MILLISECONDS);
        }
        System.out.printf("Started %d named churn threads (jcc-lab-churn-N)%n", config.workerThreads());
    }

    private void safeChurn() {
        try {
            allocator.churnOnce();
        } catch (RuntimeException error) {
            System.err.println("Churn failed: " + error.getMessage());
        }
    }

    public void stop() {
        if (workers != null) workers.shutdownNow();
    }
}
