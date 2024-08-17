// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk
// Warning Nushell version >=0.97


pub fn source<T>(_tmpdir: Option<&T>) -> String
where
    T: AsRef<str>,
    {
        r#"
        $env.config = {
        show_banner: false,
        }
        $env.SPFS_SHELL_MESSAGE? | print
    "#
        .to_string()
    }
    
