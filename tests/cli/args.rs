use super::*;

#[test]
fn legacy_and_explicit_download_syntax_both_parse() {
    let legacy = Cli::try_parse_from(["rustdm", "https://example.test/a", "-o", "a.bin"]).unwrap();
    assert!(legacy.command.is_none());
    assert_eq!(
        legacy.direct.into_options().unwrap().url(),
        "https://example.test/a"
    );
    let explicit = Cli::try_parse_from([
        "rustdm",
        "download",
        "https://example.test/a",
        "-o",
        "a.bin",
    ])
    .unwrap();
    assert!(matches!(explicit.command, Some(Command::Download(_))));
}

#[test]
fn state_directory_is_global_and_queue_commands_need_no_download_arguments() {
    for args in [
        vec!["rustdm", "--state-dir", "jobs", "queue", "list", "--json"],
        vec!["rustdm", "queue", "list", "--state-dir", "jobs", "--json"],
    ] {
        let cli = Cli::try_parse_from(args).unwrap();
        assert_eq!(cli.state_dir, PathBuf::from("jobs"));
        assert!(matches!(
            cli.command,
            Some(Command::Queue {
                action: QueueCommand::List { json: true }
            })
        ));
    }
    assert!(Cli::try_parse_from(["rustdm", "history", "--json"]).is_ok());
    assert!(Cli::try_parse_from(["rustdm", "download"]).is_err());
}
