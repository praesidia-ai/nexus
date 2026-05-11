use criterion::{criterion_group, criterion_main, Criterion};
use nexus_bench::open_bench_db;
use nexus_store::ProjectService;

fn bench_insert_message(c: &mut Criterion) {
    let conn = open_bench_db();
    let svc = ProjectService::new(&conn);
    let proj = svc.create_project("BenchProject", None, "default").unwrap();
    let conv = svc.create_conversation(&proj.id).unwrap();

    c.bench_function("store/insert_message", |b| {
        b.iter(|| {
            svc.append_nexus_message(&conv.id, "user", "Hello, benchmark world!", None)
                .unwrap();
        });
    });
}

fn bench_query_messages(c: &mut Criterion) {
    let conn = open_bench_db();
    let svc = ProjectService::new(&conn);
    let proj = svc.create_project("QueryProject", None, "default").unwrap();
    let conv = svc.create_conversation(&proj.id).unwrap();

    for i in 0..200 {
        svc.append_nexus_message(&conv.id, "user", &format!("Message {i}"), None)
            .unwrap();
    }

    c.bench_function("store/query_last_messages", |b| {
        b.iter(|| {
            let msgs = svc.list_messages(&conv.id).unwrap();
            assert!(msgs.len() >= 50);
        });
    });
}

fn bench_insert_project(c: &mut Criterion) {
    let conn = open_bench_db();
    let svc = ProjectService::new(&conn);

    c.bench_function("store/insert_project", |b| {
        b.iter(|| {
            svc.create_project("Bench Project", Some("A benchmark project"), "default")
                .unwrap();
        });
    });
}

fn bench_query_project(c: &mut Criterion) {
    let conn = open_bench_db();
    let svc = ProjectService::new(&conn);
    let proj = svc.create_project("LookupProject", None, "default").unwrap();
    let pid = proj.id.clone();

    c.bench_function("store/query_project_by_id", |b| {
        b.iter(|| {
            let p = svc.get_project(&pid).unwrap();
            assert!(p.is_some());
        });
    });
}

fn bench_bulk_insert_100_messages(c: &mut Criterion) {
    c.bench_function("store/bulk_insert_100_messages", |b| {
        b.iter_with_setup(
            || {
                let conn = open_bench_db();
                let svc_conn = open_bench_db();
                // We need a single connection for setup + bench, so just use one
                drop(conn);
                svc_conn
            },
            |conn| {
                let svc = ProjectService::new(&conn);
                let proj = svc.create_project("BulkProject", None, "default").unwrap();
                let conv = svc.create_conversation(&proj.id).unwrap();
                for i in 0..100 {
                    svc.append_nexus_message(
                        &conv.id,
                        if i % 2 == 0 { "user" } else { "assistant" },
                        &format!("Bulk message {i} with some content for realism"),
                        None,
                    )
                    .unwrap();
                }
            },
        );
    });
}

fn bench_concurrent_reads(c: &mut Criterion) {
    let rt = nexus_bench::bench_runtime();
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("concurrent.db");
    let conn = nexus_store::open_connection(&db_path).unwrap();
    let svc = ProjectService::new(&conn);
    let proj = svc.create_project("ConcurrentProject", None, "default").unwrap();
    let conv = svc.create_conversation(&proj.id).unwrap();
    for i in 0..100 {
        svc.append_nexus_message(&conv.id, "user", &format!("Msg {i}"), None)
            .unwrap();
    }
    drop(conn);

    let db_path_clone = db_path.clone();
    let conv_id = conv.id.clone();

    c.bench_function("store/concurrent_10_reads", |b| {
        b.iter(|| {
            rt.block_on(async {
                let mut handles = Vec::new();
                for _ in 0..10 {
                    let p = db_path_clone.clone();
                    let cid = conv_id.clone();
                    handles.push(tokio::spawn(async move {
                        let conn = nexus_store::open_connection(&p).unwrap();
                        let svc = ProjectService::new(&conn);
                        let msgs = svc.list_messages(&cid).unwrap();
                        assert!(!msgs.is_empty());
                    }));
                }
                for h in handles {
                    h.await.unwrap();
                }
            });
        });
    });
}

criterion_group!(
    benches,
    bench_insert_message,
    bench_query_messages,
    bench_insert_project,
    bench_query_project,
    bench_bulk_insert_100_messages,
    bench_concurrent_reads,
);
criterion_main!(benches);
