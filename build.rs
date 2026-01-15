fn main() {
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("src/logo.ico");
        res.compile().unwrap();
    }
}
