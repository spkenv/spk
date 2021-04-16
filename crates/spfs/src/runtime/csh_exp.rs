// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

pub static SOURCE: &str = r#"
set shell [lindex $argv 0]
set startup_script [lindex $argv 1]
spawn $shell -f
expect {
    > {
        send "source '${startup_script}'\n"
    }
}
interact
catch wait result
exit [lindex $result 3]
"#;
