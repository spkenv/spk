// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use clap::Parser;

use spk::fixtures::*;

use super::{Ls, Output, Run};

#[derive(Default)]
struct OutputToVec {
    vec: Vec<String>,
}

impl Output for OutputToVec {
    fn println(&mut self, line: String) {
        self.vec.push(line);
    }
}

#[derive(Parser)]
struct Opt {
    #[clap(flatten)]
    ls: Ls<OutputToVec>,
}

#[tokio::test]
async fn test_ls_trivially_works() {
    let _rt = spfs_runtime().await;

    let mut opt = Opt::try_parse_from([] as [&str; 0]).unwrap();
    opt.ls.run().await.unwrap();
    assert_eq!(opt.ls.output.vec.len(), 0);
}
