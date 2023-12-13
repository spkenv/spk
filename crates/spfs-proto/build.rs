use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=schema/spfs.fbs");

    let cmd = match std::env::var_os("FLATC") {
        Some(exe) => flatc_rust::Flatc::from_path(exe),
        None => flatc_rust::Flatc::from_env_path(),
    };

    cmd.run(flatc_rust::Args {
        lang: "rust",
        inputs: &[Path::new("schema/spfs.fbs")],
        out_dir: Path::new("src/"),
        ..Default::default()
    })
    .expect("schema compiler command");
}
