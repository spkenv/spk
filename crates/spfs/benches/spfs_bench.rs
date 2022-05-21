// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use criterion::{criterion_group, criterion_main, Criterion, SamplingMode, Throughput};
use spfs::prelude::*;

use std::{
    fs::File,
    io::{BufWriter, Write},
    time::Duration,
};
use tempdir::TempDir;

pub fn commit_benchmark(c: &mut Criterion) {
    const NUM_FILES: usize = 1024;
    const NUM_LINES: usize = 1024;

    // Populate a directory with contents to use to commit to spfs.
    let tempdir = TempDir::new("spfs-test-").expect("create a temp directory for test files");
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
    let repo_path = TempDir::new("spfs-test-repo-").expect("create a temp directory for spfs repo");
    let repo = tokio_runtime
        .block_on(spfs::storage::fs::FSRepository::create(
            repo_path.path().join("repo"),
        ))
        .expect("create spfs repo");

    let mut group = c.benchmark_group("spfs commit path");
    // use `Flat` because this is a long-running benchmark.
    group.sampling_mode(SamplingMode::Flat);
    group.throughput(Throughput::Elements(NUM_FILES as u64));
    group
        .significance_level(0.1)
        .warm_up_time(Duration::from_secs(10))
        .sample_size(10)
        .measurement_time(Duration::from_secs(200));
    group.bench_function("repo.commit_dir", |b| {
        b.to_async(&tokio_runtime)
            .iter(|| repo.commit_dir(tempdir.path()))
    });
    group.finish();
}

criterion_group!(benches, commit_benchmark);
criterion_main!(benches);
