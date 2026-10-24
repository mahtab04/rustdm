use super::*;

#[test]
fn a_busy_preferences_store_returns_an_error_without_blocking_or_losing_data() {
    let temp = tempfile::tempdir().unwrap();
    let store = PreferencesStore::new(temp.path().to_owned());
    store.save(&Preferences::default()).unwrap();
    let lock = fs::OpenOptions::new()
        .write(true)
        .open(temp.path().join("preferences.lock"))
        .unwrap();
    fs2::FileExt::lock_exclusive(&lock).unwrap();
    let updated = Preferences {
        connections: 12,
        ..Preferences::default()
    };
    assert!(
        store
            .save(&updated)
            .unwrap_err()
            .to_string()
            .contains("another window")
    );
    assert_eq!(store.load().unwrap().connections, 4);
    drop(lock);
    store.save(&updated).unwrap();
    assert_eq!(store.load().unwrap().connections, 12);
}

#[test]
fn saved_defaults_survive_restart_and_create_a_clean_download_form() {
    let temp = tempfile::tempdir().unwrap();
    let store = PreferencesStore::new(temp.path().to_owned());
    assert_eq!(store.load().unwrap(), Preferences::default());
    let preferences = Preferences {
        download_directory: temp.path().display().to_string(),
        connections: 8,
        limit_kib: Some(1024),
        resume: true,
        ..Preferences::default()
    };
    store.save(&preferences).unwrap();
    let loaded = PreferencesStore::new(temp.path().to_owned())
        .load()
        .unwrap();
    assert_eq!(loaded, preferences);
    let form = loaded.form();
    assert_eq!(form.connections, 8);
    assert_eq!(form.rate, "1024");
    assert!(form.resume);
    assert!(form.url.is_empty() && form.output.is_empty() && form.sha256.is_empty());
    // Replace an existing snapshot as well as creating the initial file.
    store.save(&Preferences::default()).unwrap();
    assert_eq!(store.load().unwrap(), Preferences::default());
}

#[test]
fn malformed_future_and_invalid_preferences_are_preserved() {
    let temp = tempfile::tempdir().unwrap();
    let store = PreferencesStore::new(temp.path().to_owned());
    for original in [
        "broken JSON",
        r#"{"version":2}"#,
        r#"{"connections":0}"#,
        r#"{"connections":17}"#,
        r#"{"limit_kib":0}"#,
        r#"{"limit_kib":18446744073709551615}"#,
    ] {
        fs::write(temp.path().join("preferences.json"), original).unwrap();
        assert!(store.load().is_err());
        assert!(store.save(&Preferences::default()).is_err());
        assert_eq!(
            fs::read_to_string(temp.path().join("preferences.json")).unwrap(),
            original
        );
    }
}

#[test]
fn partial_preferences_use_defaults_and_invalid_updates_leave_the_snapshot_intact() {
    let temp = tempfile::tempdir().unwrap();
    let store = PreferencesStore::new(temp.path().to_owned());
    fs::write(temp.path().join("preferences.json"), r#"{"connections":6}"#).unwrap();
    let mut loaded = store.load().unwrap();
    assert_eq!(loaded.connections, 6);
    assert_eq!(loaded.limit_kib, None);
    loaded.limit_kib = Some(0);
    assert!(store.save(&loaded).is_err());
    assert_eq!(store.load().unwrap().connections, 6);
}
