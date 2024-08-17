// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk
// Warning Nushell version >=0.97

pub fn source<T>(_tmpdir: Option<&T>) -> String
where
    T: AsRef<str>,
    {
        r#"
        def create_left_prompt [] {
            let dir = match (do --ignore-shell-errors { $env.PWD | path relative-to $nu.home-path }) {
                null => $env.PWD
                '' => '~'
                $relative_pwd => ([~ $relative_pwd] | path join)
            }

            let path_color = (if (is-admin) { ansi red_bold } else { ansi green_bold })
            let separator_color = (if (is-admin) { ansi light_red_bold } else { ansi light_green_bold })
            let path_segment = $"($path_color)($dir)(ansi reset)"

            $path_segment | str replace --all (char path_sep) $"($separator_color)(char path_sep)($path_color)"
        }

        def create_right_prompt [] {
            # create a right prompt in magenta with green separators and am/pm underlined
            let time_segment = ([
                (ansi reset)
                (ansi magenta)
                (date now | format date '%x %X') # try to respect user's locale
            ] | str join | str replace --regex --all "([/:])" $"(ansi green)${1}(ansi magenta)" |
                str replace --regex --all "([AP]M)" $"(ansi magenta_underline)${1}")

            let last_exit_code = if ($env.LAST_EXIT_CODE != 0) {([
                (ansi rb)
                ($env.LAST_EXIT_CODE)
            ] | str join)
            } else { "" }

            ([$last_exit_code, (char space), $time_segment] | str join)
        }

        # Use nushell functions to define your right and left prompt
        $env.PROMPT_COMMAND = {|| create_left_prompt }
        # FIXME: This default is not implemented in rust code as of 2023-09-08.
        $env.PROMPT_COMMAND_RIGHT = {|| create_right_prompt }

        # The prompt indicators are environmental variables that represent
        # the state of the prompt
        $env.PROMPT_INDICATOR = {|| "> " }
        $env.PROMPT_INDICATOR_VI_INSERT = {|| ": " }
        $env.PROMPT_INDICATOR_VI_NORMAL = {|| "> " }
        $env.PROMPT_MULTILINE_INDICATOR = {|| "::: " }


        $env.ENV_CONVERSIONS = {
            "PATH": {
                from_string: { |s| $s | split row (char esep) | path expand --no-symlink }
                to_string: { |v| $v | path expand --no-symlink | str join (char esep) }
            }
            "Path": {
                from_string: { |s| $s | split row (char esep) | path expand --no-symlink }
                to_string: { |v| $v | path expand --no-symlink | str join (char esep) }
            }
        }

        let $spfs_startup_dir = if $nu.os-info.name == "windows" {
            "C:/spfs/etc/spfs/startup.d"
        } else if $nu.os-info.name == "linux" {
            "/spfs/etc/spfs/startup.d"
        } else {
            exit 1
        }

        $env.NU_VENDOR_AUTOLOAD_DIR = ($spfs_startup_dir)
    "#
        .to_string()
    }
    