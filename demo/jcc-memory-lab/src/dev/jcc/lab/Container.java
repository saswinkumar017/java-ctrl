package dev.jcc.lab;

import java.lang.reflect.Constructor;
import java.lang.reflect.Field;
import java.util.LinkedHashSet;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;

/**
 * A deliberately small reflection-based dependency injection container.
 * It eagerly creates every registered singleton and injects annotated fields,
 * which gives JCC's Class Loading Explorer / JIT Explorer plenty to show.
 */
public final class Container {
    private final Set<Class<?>> components;
    private final Map<Class<?>, Object> instances = new ConcurrentHashMap<>();

    public Container(Class<?>... components) {
        this.components = new LinkedHashSet<>(Set.of(components));
    }

    public void registerInstance(Class<?> type, Object instance) {
        instances.put(type, instance);
    }

    public void init() {
        for (Class<?> type : components) {
            if (type.isAnnotationPresent(Singleton.class) || type.isAnnotationPresent(Component.class)) {
                get(type);
            }
        }
    }

    public <T> T get(Class<T> type) {
        if (type.isAnnotationPresent(Singleton.class)) {
            return type.cast(instances.computeIfAbsent(type, this::create));
        }
        return type.cast(create(type));
    }

    private Object create(Class<?> type) {
        try {
            Constructor<?> constructor = type.getDeclaredConstructor();
            constructor.setAccessible(true);
            Object bean = constructor.newInstance();
            for (Field field : type.getDeclaredFields()) {
                Inject inject = field.getAnnotation(Inject.class);
                if (inject == null) continue;
                Class<?> dependency = inject.value() == void.class ? field.getType() : inject.value();
                field.setAccessible(true);
                field.set(bean, get(dependency));
            }
            return bean;
        } catch (ReflectiveOperationException error) {
            throw new IllegalStateException("Could not create bean " + type.getName(), error);
        }
    }
}
