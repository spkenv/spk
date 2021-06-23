// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

#[macro_export]
macro_rules! fixtures {
    () => {
        #[allow(dead_code)]
        fn init_logging() -> tracing::dispatcher::DefaultGuard {
            let sub = tracing_subscriber::FmtSubscriber::builder()
                .with_max_level(tracing::Level::TRACE)
                .without_time()
                .with_test_writer()
                .finish();
            tracing::subscriber::set_default(sub)
        }
    };
}
