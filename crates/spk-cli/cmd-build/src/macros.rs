// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use clap::Parser;

use crate::cmd_build::Build;

#[derive(Parser)]
pub struct BuildOpt {
    #[clap(flatten)]
    pub build: Build,
}

#[macro_export]
macro_rules! try_build_package {
    ($tmpdir:ident, $filename:literal, $recipe:expr, $($extra_build_args:literal),*) => {{
        // Leak `filename` for convenience.
        let filename = Box::leak(Box::new($tmpdir.path().join($filename)));
        {
            let mut file = File::create(&filename).unwrap();
            file.write_all($recipe).unwrap();
        }

        let filename_str = filename.as_os_str().to_str().unwrap();

        $crate::try_build_package!($tmpdir, filename_str, $($extra_build_args),*)
    }};

    ($tmpdir:ident, $filename:literal, $recipe:literal $(,)? $($extra_build_args:literal),*) => {{
        $crate::try_build_package!(
            $tmpdir,
            $filename,
            $recipe,
            $($extra_build_args,)*
        )
    }};

    ($tmpdir:ident, $filename:ident $(,)? $($extra_build_args:literal),*) => {{
        // Build the package so it can be tested.
        let mut opt = $crate::macros::BuildOpt::try_parse_from([
            "build",
            // Don't exec a new process to move into a new runtime, this confuses
            // coverage testing.
            "--no-runtime",
            "--disable-repo=origin",
            $($extra_build_args,)*
            $filename,
        ])
        .unwrap();

        opt.build.run().await.map(|_| $filename)
    }};
}

#[macro_export]
macro_rules! build_package {
    ($($args:tt)*) => {{
        $crate::try_build_package!($($args)*).unwrap()
    }};
}
