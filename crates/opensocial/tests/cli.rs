//! The command as people run it.

use std::process::Command;

fn opensocial(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_opensocial"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn prints_usage() {
    for args in [&[][..], &["help"], &["--help"], &["-h"]] {
        let out = opensocial(args);
        assert!(out.status.success());
        assert_eq!(String::from_utf8(out.stdout).unwrap(), opensocial::USAGE);
    }
}

#[test]
fn refuses_an_unknown_command() {
    let out = opensocial(&["frobnicate"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).starts_with("no command frobnicate"));
}

#[test]
fn reads_who_it_is_from_the_environment() {
    let out = Command::new(env!("CARGO_BIN_EXE_opensocial"))
        .args(["inspect", "riders.groups.test"])
        .env_remove("OPENSOCIAL_AS")
        .env_remove("OPENSOCIAL_PASSWORD")
        .output()
        .unwrap();
    assert!(!out.status.success());
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(
        said.contains("set OPENSOCIAL_AS and OPENSOCIAL_PASSWORD"),
        "{said}"
    );
}
