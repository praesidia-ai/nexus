use criterion::{criterion_group, criterion_main, Criterion};
use nexus_bench::bench_runtime;
use nexus_kernel::events::{EventBus, EventBusConfig};
use nexus_kernel::reactive_manager::ReactiveAgentManager;
use nexus_kernel::scheduler::{AgentScheduler, SchedulerConfig};
use nexus_memory::MemorySystem;
use std::sync::Arc;

fn bench_sqlite_open_and_migrate(c: &mut Criterion) {
    c.bench_function("cold_start/sqlite_open_migrate_24", |b| {
        b.iter_with_setup(
            || {
                let dir = tempfile::tempdir().unwrap();
                let db_path = dir.path().join("bench.db");
                (dir, db_path)
            },
            |(_dir, db_path)| {
                let _conn = nexus_store::open_connection(&db_path).unwrap();
            },
        );
    });
}

fn bench_memory_system_init(c: &mut Criterion) {
    c.bench_function("cold_start/memory_system_init", |b| {
        b.iter_with_setup(
            || tempfile::tempdir().unwrap(),
            |dir| {
                let _sys = MemorySystem::new(dir.path()).unwrap();
            },
        );
    });
}

fn bench_kernel_init(c: &mut Criterion) {
    let rt = bench_runtime();

    c.bench_function("cold_start/kernel_init", |b| {
        b.iter(|| {
            rt.block_on(async {
                let scheduler = Arc::new(AgentScheduler::new(SchedulerConfig::default()));
                let event_bus = Arc::new(EventBus::new(EventBusConfig::default()));
                let _reactive_mgr =
                    ReactiveAgentManager::new(scheduler.clone(), event_bus.clone());
            });
        });
    });
}

criterion_group!(
    benches,
    bench_sqlite_open_and_migrate,
    bench_memory_system_init,
    bench_kernel_init,
);
criterion_main!(benches);
