use std::io::Write;
use std::os::unix::fs::PermissionsExt;

fn main() {
    // Open a file to write package spec into
    let mut file = std::fs::File::create("sudoku-pkgs/build.sh").unwrap();
    let mut build_script = std::io::BufWriter::new(&mut file);
    writeln!(build_script, "#!/bin/sh").unwrap();
    writeln!(build_script).unwrap();

    for row in 1..=9 {
        let our_vertical_tile = (row - 1) / 3;

        for col in 1..=9 {
            let our_horizontal_tile = (col - 1) / 3;

            // Open a file to write package spec into
            let filename = format!("sudoku-pkgs/row{}-col{}.yaml", row, col);
            let mut file = std::fs::File::create(&filename).unwrap();
            let mut writer = std::io::BufWriter::new(&mut file);

            writeln!(writer, "pkg: row{row}-col{col}/{{{{opt.version}}}}.0.0").unwrap();
            writeln!(writer, "api: v0/package").unwrap();
            writeln!(writer, "build:").unwrap();
            writeln!(writer, "  script:").unwrap();
            writeln!(writer, "    - true").unwrap();
            writeln!(writer, "install:").unwrap();
            writeln!(writer, "  requirements:").unwrap();

            for other_row in 1..=9 {
                let other_vertical_tile = (other_row - 1) / 3;

                for other_col in 1..=9 {
                    let other_horizontal_tile = (other_col - 1) / 3;

                    if other_row == row && other_col == col {
                        continue;
                    }

                    if other_row == row
                        || other_col == col
                        || other_horizontal_tile == our_horizontal_tile
                            && other_vertical_tile == our_vertical_tile
                    {
                        writeln!(
                            writer,
                            "    - pkg: row{other_row}-col{other_col}/!={{{{opt.version}}}}.0.0"
                        )
                        .unwrap();
                    }
                }
            }
            writeln!(writer).unwrap();
            for version in 1..=9 {
                writeln!(
                    build_script,
                    "spk build -o version={version} row{row}-col{col}.yaml"
                )
                .unwrap();
            }
        }
    }

    {
        let mut file = std::fs::File::create("sudoku-pkgs/sudoku-solution.spk.yaml").unwrap();
        let mut solution = std::io::BufWriter::new(&mut file);

        writeln!(solution, "pkg: sudoku-solution/1.0.0").unwrap();
        writeln!(solution, "api: v0/package").unwrap();
        writeln!(solution, "build:").unwrap();
        writeln!(solution, "  script:").unwrap();
        writeln!(solution, "    - true").unwrap();
        writeln!(solution, "install:").unwrap();
        writeln!(solution, "  requirements:").unwrap();
        for row in 1..=9 {
            for col in 1..=9 {
                writeln!(solution, "    - pkg: row{row}-col{col}").unwrap();
            }
        }

        writeln!(build_script).unwrap();
        writeln!(build_script, "spk build sudoku-solution.spk.yaml").unwrap();
    }

    drop(build_script);
    drop(file);

    // Make the build script executable
    std::fs::set_permissions(
        "sudoku-pkgs/build.sh",
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();

    // NYTimes easy puzzle Oct 25 2024
    let given = [
        "row1-col1/=1.0.0",
        "row1-col3/=4.0.0",
        "row1-col4/=9.0.0",
        "row1-col6/=8.0.0",
        "row1-col7/=2.0.0",
        //
        "row2-col2/=9.0.0",
        "row2-col4/=5.0.0",
        "row2-col6/=2.0.0",
        "row2-col8/=7.0.0",
        "row2-col9/=6.0.0",
        //
        "row3-col1/=2.0.0",
        "row3-col2/=7.0.0",
        "row3-col3/=5.0.0",
        "row3-col5/=1.0.0",
        "row3-col9/=4.0.0",
        //
        "row4-col4/=2.0.0",
        "row4-col5/=9.0.0",
        "row4-col6/=5.0.0",
        "row4-col7/=8.0.0",
        "row4-col8/=4.0.0",
        //
        "row5-col1/=4.0.0",
        "row5-col3/=9.0.0",
        "row5-col4/=8.0.0",
        "row5-col7/=5.0.0",
        "row5-col8/=1.0.0",
        //
        "row6-col2/=8.0.0",
        "row6-col5/=3.0.0",
        "row6-col9/=7.0.0",
        //
        "row7-col1/=3.0.0",
        "row7-col2/=2.0.0",
        "row7-col3/=1.0.0",
        "row7-col6/=9.0.0",
        "row7-col7/=4.0.0",
        //
        "row8-col7/=7.0.0",
        "row8-col8/=5.0.0",
        //
        "row9-col5/=8.0.0",
        "row9-col6/=6.0.0",
        "row9-col9/=1.0.0",
    ];

    println!("spk env {} sudoku-solution", given.join(" "));

    {
        let mut file = std::fs::File::create("sudoku-pkgs/print-solution.sh").unwrap();
        let mut solution = std::io::BufWriter::new(&mut file);

        for row in 1..=9 {
            for col in 1..=9 {
                writeln!(
                    solution,
                    "echo -n \" $SPK_PKG_row{row}_col{col}_VERSION_MAJOR \""
                )
                .unwrap();
            }
            writeln!(solution, "echo").unwrap();
        }
    }
}
