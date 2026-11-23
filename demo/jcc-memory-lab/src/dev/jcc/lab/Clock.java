package dev.jcc.lab;

/** Injected time source — makes the DI wiring visible in class histograms. */
public interface Clock {
    long nowMillis();
}
