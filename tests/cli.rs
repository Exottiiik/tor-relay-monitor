//! Both command names launch the same application without changing a service.
#[test]
fn short_and_compatibility_commands_report_version_and_help() {
    for (path, name) in [
        (env!("CARGO_BIN_EXE_trm"), "trm"),
        (env!("CARGO_BIN_EXE_tor-relay-monitor"), "tor-relay-monitor"),
    ] {
        let version = std::process::Command::new(path)
            .arg("--version")
            .output()
            .unwrap();
        assert!(version.status.success());
        assert_eq!(
            String::from_utf8(version.stdout).unwrap().trim(),
            format!("{name} {}", env!("CARGO_PKG_VERSION"))
        );
        let help = std::process::Command::new(path)
            .arg("--help")
            .output()
            .unwrap();
        assert!(help.status.success());
        let text = String::from_utf8(help.stdout).unwrap();
        assert!(text.contains("Tor Relay Monitor") && text.contains(&format!("Usage: {name}")));
    }
}
