use super::*;

#[test]
fn form_validates_destination_rate_and_checksum() {
    let temp = tempfile::tempdir().unwrap();
    let mut form = DownloadForm::default();
    assert!(form.options().is_err());
    form.url = "https://example.test/file.zip".into();
    form.output = temp.path().display().to_string();
    assert!(form.options().is_err());
    form.output = temp.path().join("file.zip").display().to_string();
    form.rate = "512".into();
    form.sha256 = "AB".repeat(32);
    let options = form.options().unwrap();
    assert_eq!(options.bandwidth_limit(), Some(512 * 1024));
    assert_eq!(options.sha256().unwrap().to_string(), "ab".repeat(32));
    for rate in ["0", "-1", "fast", "18446744073709551615"] {
        form.rate = rate.into();
        assert!(form.options().is_err());
    }
    form.rate.clear();
    form.sha256 = "invalid".into();
    assert!(form.options().is_err());
}

#[test]
fn filters_include_paused_jobs_and_keep_failures_visible_in_all() {
    assert!(Filter::All.accepts(JobState::Failed));
    assert!(Filter::Active.accepts(JobState::Paused));
    assert!(Filter::Active.accepts(JobState::Pending));
    assert!(!Filter::Active.accepts(JobState::Completed));
    assert!(Filter::Completed.accepts(JobState::Completed));
    assert!(!Filter::Completed.accepts(JobState::Running));
}
