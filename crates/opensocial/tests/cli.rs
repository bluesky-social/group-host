//! The command as people run it.

use serde_json::json;
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
        .args(["group", "show", "riders.groups.test"])
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

fn tool(name: &str) -> group_host_mcp::Tool {
    let tools = group_host_mcp::tools();
    tools.into_iter().find(|t| t.name == name).unwrap()
}

#[test]
fn reads_a_commands_words_as_its_tools_arguments() {
    let read = |name: &str, words: &[&str]| opensocial::arguments(&tool(name), words);
    // The group first, then what the tool takes in order; a list split.
    let (group, args) = read(
        "member_assign",
        &["riders.test", "bob.test", "member,admin,"],
    )
    .unwrap();
    assert_eq!(group, "riders.test");
    assert_eq!(
        args,
        json!({"member": "bob.test", "roles": ["member", "admin"]})
    );
    // Flags in kebab case: yes, no, a number, JSON.
    let flags = [
        "g",
        "leader",
        "--display-name",
        "Ride leader",
        "--bot",
        "--no-default",
    ];
    let (_, args) = read("role_set", &flags).unwrap();
    assert_eq!(
        args,
        json!({"role": "leader", "display_name": "Ride leader", "bot": true, "default": false})
    );
    let (_, args) = read("join_list", &["g", "--limit", "5"]).unwrap();
    assert_eq!(args, json!({"limit": 5}));
    let access = r#"{"grants": []}"#;
    let flags = ["g", "com.example.forum", "--access", access, "--no-index"];
    let (_, args) = read("space_create", &flags).unwrap();
    assert_eq!(
        args,
        json!({"type": "com.example.forum", "access": {"grants": []}, "index": false})
    );

    let wrong = |name: &str, words: &[&str]| format!("{}", read(name, words).unwrap_err());
    assert!(wrong("member_eject", &["g"]).starts_with("usage:"));
    assert!(wrong("group_show", &[]).starts_with("usage:"));
    assert_eq!(
        wrong("join_list", &["g", "--limit", "x"]),
        "--limit wants a number"
    );
    assert_eq!(
        wrong("join_list", &["g", "--limit"]),
        "--limit wants a value"
    );
    let flags = ["g", "t", "--access", "{"];
    assert_eq!(wrong("space_create", &flags), "--access wants JSON");
    assert!(wrong("join_list", &["g", "--colour", "red"]).starts_with("no option --colour"));
    assert!(wrong("join_list", &["g", "--no-limit"]).starts_with("no option --no-limit"));
    // What it takes in order is never a flag.
    assert!(wrong("member_eject", &["g", "--member", "m"]).starts_with("no option --member"));
}

#[test]
fn lists_every_command_in_its_usage() {
    for t in group_host_mcp::tools() {
        let line = format!("\n  {} <group>", t.command());
        assert!(opensocial::USAGE.contains(&line), "{}", t.name);
    }
}
