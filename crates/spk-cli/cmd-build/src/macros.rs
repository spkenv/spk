// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use clap::Parser;

use crate::cmd_build::Build;

#[derive(Parser)]
pub struct BuildOpt {
    #[clap(flatten)]
    pub build: Build,
}

#[macro_export]
macro_rules! build_package {
    ($tmpdir:ident, $filename:literal, $recipe:literal, $solver_to_run:expr $(,$extra_build_args:expr)* $(,)?) => {{
        let (filename, r) = $crate::try_build_package!($tmpdir, $filename, $recipe, $solver_to_run, $($extra_build_args),*);
        r.unwrap();
        filename
    }};

    ($tmpdir:ident, $filename:literal, $recipe:ident, $solver_to_run:expr $(,$extra_build_args:expr)* $(,)?) => {{
        let (filename, r) = $crate::try_build_package!($tmpdir, $filename, $recipe, $solver_to_run, $($extra_build_args),*);
        r.unwrap();
        filename
    }};

    ($tmpdir:ident, $filename:ident, $solver_to_run:expr $(,$extra_build_args:expr)* $(,)?) => {{
        let (filename, r) = $crate::try_build_package!($tmpdir, $filename, $solver_to_run, $($extra_build_args),*);
        r.unwrap();
        filename
    }};
}

#[macro_export]
macro_rules! try_build_package {
    ($tmpdir:ident, $filename:literal, $recipe:literal, $solver_to_run:expr $(,$extra_build_args:expr)* $(,)?) => {{
        // Leak `filename` for convenience.
        let filename = Box::leak(Box::new($tmpdir.path().join($filename)));
        {
            let mut file = std::fs::File::create(&filename).unwrap();
            use std::io::Write;
            file.write_all($recipe).unwrap();
        }

        let filename_str = filename.as_os_str().to_str().unwrap();

        $crate::try_build_package!($tmpdir, filename_str, $solver_to_run, $($extra_build_args),*)
    }};

    ($tmpdir:ident, $filename:literal, $recipe:expr, $solver_to_run:expr $(,$extra_build_args:expr)* $(,)?) => {{
        // Leak `filename` for convenience.
        let filename = Box::leak(Box::new($tmpdir.path().join($filename)));
        {
            let mut file = std::fs::File::create(&filename).unwrap();
            use std::io::Write;
            file.write_all($recipe.as_bytes()).unwrap();
        }

        let filename_str = filename.as_os_str().to_str().unwrap();

        $crate::try_build_package!($tmpdir, filename_str, $solver_to_run, $($extra_build_args),*)
    }};

    ($tmpdir:ident, $filename:ident, $solver_to_run:expr $(,$extra_build_args:expr)* $(,)?) => {{
        // Build the package so it can be tested.
        use clap::Parser;
        let mut opt = $crate::macros::BuildOpt::try_parse_from([
            "build",
            // Don't exec a new process to move into a new runtime, this confuses
            // coverage testing.
            "--no-runtime",
            "--disable-repo=origin",
            "--solver-to-run",
            $solver_to_run,
            $($extra_build_args,)*
            $filename,
        ])
        .unwrap();
        use spk_cli_common::Run;
        ($filename, opt.build.run().await)
    }};
}
