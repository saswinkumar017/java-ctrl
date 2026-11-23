package dev.jcc.lab;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.atomic.AtomicLong;

/** Holds the deterministic retained payload and accounts for every allocation. */
@Singleton
public final class MemoryStore {
    private final List<byte[]> retained = new ArrayList<>();
    private final AtomicLong allocatedBytes = new AtomicLong();
    private final AtomicLong transientBytes = new AtomicLong();

    @Inject
    private LabConfig config;

    /** Allocates one retained block and keeps a strong reference to it. */
    public synchronized byte[] retain() {
        byte[] block = new byte[config.blockBytes()];
        block[0] = 1; // touch the first page so it is really committed
        retained.add(block);
        allocatedBytes.addAndGet(block.length);
        return block;
    }

    /** Allocates a block that is immediately eligible for GC. */
    public byte[] churn() {
        byte[] block = new byte[config.blockBytes()];
        block[0] = (byte) (System.nanoTime() & 0xFF);
        allocatedBytes.addAndGet(block.length);
        transientBytes.addAndGet(block.length);
        return block;
    }

    public synchronized int retainedCount() { return retained.size(); }

    public synchronized long retainedBytes() {
        long bytes = 0;
        for (byte[] block : retained) bytes += block.length;
        return bytes;
    }

    public long allocatedBytes() { return allocatedBytes.get(); }
    public long transientBytes() { return transientBytes.get(); }

    public synchronized void clear() {
        retained.clear();
    }
}
