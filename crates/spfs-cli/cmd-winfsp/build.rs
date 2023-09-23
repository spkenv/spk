fn main() {
    #[cfg(windows)]
    winfsp::build::winfsp_link_delayload();
}
