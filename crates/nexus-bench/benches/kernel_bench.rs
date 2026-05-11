use criterion::{criterion_group, criterion_main, Criterion};

fn kernel_placeholder(c: &mut Criterion) {
    c.bench_function("kernel_noop", |b| b.iter(|| 1 + 1));
}

criterion_group!(benches, kernel_placeholder);
criterion_main!(benches);
