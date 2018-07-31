use std::path::*;
use std::process::Command;

// note: to make debug prints work:
// cargo test -- --nocapture
#[cfg(test)]
mod sizetests {
    use super::*;

    #[test]
    fn build_and_check_size_test() {
        // move into the directory of our dummy crate
        // set a fake CARGO_HOME and build the dummy crate there
        let crate_path = PathBuf::from("tests/size_test/");
        let status = Command::new("cargo")
            .arg("check")
            .current_dir(&crate_path)
            .env("CARGO_HOME", "fake_cargo_home")
            .output();
        // make sure the build succeeded
        assert!(status.is_ok(), "build of dummy crate did not succeed");
        assert!(
            PathBuf::from("tests/size_test/fake_cargo_home").is_dir(),
            "fake cargo home was not created!"
        );
    }
}