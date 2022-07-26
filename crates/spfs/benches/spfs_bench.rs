// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use spfs::prelude::*;

use std::{
    fs::File,
    io::{BufWriter, Write},
    sync::Arc,
    time::Duration,
};

pub fn commit_benchmark(c: &mut Criterion) {
    const NUM_FILES: usize = 1024;
    const NUM_LINES: usize = 1024;

    // Populate a directory with contents to use to commit to spfs.
    let tempdir = tempfile::Builder::new()
        .prefix("spfs-test-")
        .tempdir()
        .expect("create a temp directory for test files");
    let mut content: usize = 0;
    for filename in 0..NUM_FILES {
        let mut f = BufWriter::new(
            File::create(tempdir.path().join(filename.to_string())).expect("open file for writing"),
        );
        for _ in 0..NUM_LINES {
            f.write_all(content.to_string().as_ref())
                .expect("write to file");
            content += 1;
        }
        f.flush().expect("write all contents");
    }

    let tokio_runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("create tokio runtime");

    // Create an spfs repo to commit this path to.
    let repo_path = tempfile::Builder::new()
        .prefix("spfs-test-repo-")
        .tempdir()
        .expect("create a temp directory for spfs repo");
    let repo: Arc<RepositoryHandle> = Arc::new(
        tokio_runtime
            .block_on(spfs::storage::fs::FSRepository::create(
                repo_path.path().join("repo"),
            ))
            .expect("create spfs repo")
            .into(),
    );

    let mut group = c.benchmark_group("spfs commit path");
    group.throughput(Throughput::Elements(NUM_FILES as u64));
    group
        .significance_level(0.1)
        .sample_size(20)
        .measurement_time(Duration::from_secs(10));
    group.bench_function("repo.commit_dir", |b| {
        b.to_async(&tokio_runtime)
            .iter(|| spfs::commit_dir(Arc::clone(&repo), tempdir.path()))
    });
    group.finish();
}

criterion_group!(benches, commit_benchmark);
criterion_main!(benches);
