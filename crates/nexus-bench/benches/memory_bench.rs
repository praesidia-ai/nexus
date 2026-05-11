use criterion::{criterion_group, criterion_main, Criterion};
use nexus_bench::{random_embedding, sample_episode, sample_fact};
use nexus_memory::vector_store::{cosine_similarity, top_k_similar};
use nexus_memory::MemorySystem;

fn bench_episodic_record(c: &mut Criterion) {
    let dir = tempfile::tempdir().unwrap();
    let sys = MemorySystem::new(dir.path()).unwrap();
    let mut counter = 0u64;

    c.bench_function("memory/episodic_record", |b| {
        b.iter(|| {
            counter += 1;
            let ep = sample_episode(
                &format!("ep-{counter}"),
                "bench-proj",
                0.7,
                random_embedding(64, counter),
            );
            sys.episodic.record(&ep).unwrap();
        });
    });
}

fn bench_episodic_recall(c: &mut Criterion) {
    let dir = tempfile::tempdir().unwrap();
    let sys = MemorySystem::new(dir.path()).unwrap();

    for i in 0..200 {
        let ep = sample_episode(
            &format!("ep-{i}"),
            "bench-proj",
            0.5 + (i as f32 / 400.0),
            random_embedding(64, i),
        );
        sys.episodic.record(&ep).unwrap();
    }

    c.bench_function("memory/episodic_recall", |b| {
        b.iter(|| {
            let episodes = sys
                .episodic
                .recent_by_project("bench-proj", None, &[], 20)
                .unwrap();
            assert!(!episodes.is_empty());
        });
    });
}

fn bench_semantic_store(c: &mut Criterion) {
    let dir = tempfile::tempdir().unwrap();
    let sys = MemorySystem::new(dir.path()).unwrap();
    let mut counter = 0u64;

    c.bench_function("memory/semantic_store", |b| {
        b.iter(|| {
            counter += 1;
            let fact = sample_fact(
                &format!("f-{counter}"),
                "bench-tenant",
                random_embedding(64, counter * 31),
            );
            sys.semantic.learn(&fact).unwrap();
        });
    });
}

fn bench_semantic_search(c: &mut Criterion) {
    let dir = tempfile::tempdir().unwrap();
    let sys = MemorySystem::new(dir.path()).unwrap();

    for i in 0..200u64 {
        let mut fact = sample_fact(
            &format!("f-{i}"),
            "bench-tenant",
            random_embedding(64, i * 31),
        );
        fact.subject = format!("Subject {i}");
        sys.semantic.learn(&fact).unwrap();
    }

    let query_emb = random_embedding(64, 9999);

    c.bench_function("memory/semantic_search", |b| {
        b.iter(|| {
            let results = sys.semantic.query(&query_emb, None, 10).unwrap();
            assert!(!results.is_empty());
        });
    });
}

fn bench_vector_cosine_similarity(c: &mut Criterion) {
    let a = random_embedding(1536, 42);
    let b = random_embedding(1536, 84);

    c.bench_function("memory/vector_cosine_1536dim", |b_iter| {
        b_iter.iter(|| {
            let _sim = cosine_similarity(&a, &b);
        });
    });
}

fn bench_vector_topk_search(c: &mut Criterion) {
    let candidates: Vec<Vec<f32>> = (0..1000)
        .map(|i| random_embedding(1536, i))
        .collect();
    let query = random_embedding(1536, 9999);

    c.bench_function("memory/vector_topk_1000x1536", |b| {
        b.iter(|| {
            let results = top_k_similar(&query, &candidates, 10);
            assert_eq!(results.len(), 10);
        });
    });
}

criterion_group!(
    benches,
    bench_episodic_record,
    bench_episodic_recall,
    bench_semantic_store,
    bench_semantic_search,
    bench_vector_cosine_similarity,
    bench_vector_topk_search,
);
criterion_main!(benches);
