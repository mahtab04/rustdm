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

#[test]
fn bandwidth_limit_parses_for_direct_and_queued_downloads() {
    for prefix in [vec![], vec!["download"], vec!["queue", "add"]] {
        let mut args = vec!["rustdm"];
        args.extend(prefix);
        args.extend([
            "https://example.test/a",
            "-o",
            "a.bin",
            "--limit-rate",
            "65536",
        ]);
        let cli = Cli::try_parse_from(args).unwrap();
        let options = match cli.command {
            None => cli.direct.into_options(),
            Some(Command::Download(args))
            | Some(Command::Queue {
                action: QueueCommand::Add(args),
            }) => args.into_options(),
            _ => panic!("Unexpected command"),
        }
        .unwrap();
        assert_eq!(options.bandwidth_limit(), Some(65536));
    }
    for invalid in ["0", "-1", "1MiB", "18446744073709551616"] {
        assert!(
            Cli::try_parse_from([
                "rustdm",
                "https://example.test/a",
                "-o",
                "a",
                "--limit-rate",
                invalid
            ])
            .is_err()
        );
    }
    let cli = Cli::try_parse_from(["rustdm", "--limit-rate", "1", "queue", "run"]).unwrap();
    assert!(cli.direct.ensure_no_overrides().is_err());
}

#[test]
fn sha256_parses_for_legacy_explicit_and_queued_downloads() {
    let digest = "AB".repeat(32);
    for prefix in [vec![], vec!["download"], vec!["queue", "add"]] {
        let mut args = vec!["rustdm"];
        args.extend(prefix);
        args.extend(["https://example.test/a", "-o", "a.bin", "--sha256", &digest]);
        let cli = Cli::try_parse_from(args).unwrap();
        let options = match cli.command {
            None => cli.direct.into_options(),
            Some(Command::Download(args))
            | Some(Command::Queue {
                action: QueueCommand::Add(args),
            }) => args.into_options(),
            _ => panic!("Unexpected command"),
        }
        .unwrap();
        assert_eq!(
            options.sha256().unwrap().to_string(),
            digest.to_ascii_lowercase()
        );
    }
    for invalid in ["short", &"g".repeat(64), &"a".repeat(65)] {
        assert!(
            Cli::try_parse_from([
                "rustdm",
                "https://example.test/a",
                "-o",
                "a",
                "--sha256",
                invalid
            ])
            .is_err()
        );
    }
    let cli = Cli::try_parse_from(["rustdm", "--sha256", &digest, "queue", "run"]).unwrap();
    assert!(cli.direct.ensure_no_overrides().is_err());
}
