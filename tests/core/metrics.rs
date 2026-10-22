use super::*;

#[test]
fn resumed_bytes_are_not_counted_as_new_transfer_speed() {
    let mut meter = ProgressMeter::new();
    let start = Instant::now();
    let initial = meter.observe_at(
        Progress {
            downloaded: 1000,
            total: Some(2000),
        },
        start,
    );
    assert!(initial.bytes_per_second.is_none());
    let sample = meter.observe_at(
        Progress {
            downloaded: 1500,
            total: Some(2000),
        },
        start + Duration::from_secs(2),
    );
    assert_eq!(sample.bytes_per_second, Some(250.0));
    assert_eq!(sample.eta, Some(Duration::from_secs(2)));
}

#[test]
fn unknown_size_has_speed_but_no_eta() {
    let mut meter = ProgressMeter::new();
    let start = Instant::now();
    meter.observe_at(
        Progress {
            downloaded: 0,
            total: None,
        },
        start,
    );
    let sample = meter.observe_at(
        Progress {
            downloaded: 500,
            total: None,
        },
        start + Duration::from_secs(2),
    );
    assert_eq!(sample.bytes_per_second, Some(250.0));
    assert!(sample.eta.is_none());
}

#[test]
fn completed_and_empty_files_have_zero_eta() {
    let sample = ProgressMeter::new().observe(Progress {
        downloaded: 0,
        total: Some(0),
    });
    assert_eq!(sample.eta, Some(Duration::ZERO));
    assert!(sample.bytes_per_second.is_none());
}

#[test]
fn retry_counter_reset_starts_a_new_measurement() {
    let mut meter = ProgressMeter::new();
    let start = Instant::now();
    meter.observe_at(
        Progress {
            downloaded: 1500,
            total: Some(2000),
        },
        start,
    );
    let reset = meter.observe_at(
        Progress {
            downloaded: 100,
            total: Some(2000),
        },
        start + Duration::from_secs(1),
    );
    assert!(reset.bytes_per_second.is_none());
    assert!(reset.eta.is_none());
    let sample = meter.observe_at(
        Progress {
            downloaded: 300,
            total: Some(2000),
        },
        start + Duration::from_secs(2),
    );
    assert_eq!(sample.bytes_per_second, Some(200.0));
}

#[test]
fn same_timestamp_does_not_invent_a_rate_or_lose_bytes() {
    let mut meter = ProgressMeter::new();
    let start = Instant::now();
    meter.observe_at(
        Progress {
            downloaded: 0,
            total: Some(2000),
        },
        start,
    );
    let immediate = meter.observe_at(
        Progress {
            downloaded: 500,
            total: Some(2000),
        },
        start,
    );
    assert!(immediate.bytes_per_second.is_none());
    let sample = meter.observe_at(
        Progress {
            downloaded: 1000,
            total: Some(2000),
        },
        start + Duration::from_secs(1),
    );
    assert_eq!(sample.bytes_per_second, Some(1000.0));
}

#[test]
fn no_new_bytes_means_no_eta_estimate() {
    let mut meter = ProgressMeter::new();
    let start = Instant::now();
    meter.observe_at(
        Progress {
            downloaded: 500,
            total: Some(2000),
        },
        start,
    );
    let sample = meter.observe_at(
        Progress {
            downloaded: 500,
            total: Some(2000),
        },
        start + Duration::from_secs(1),
    );
    assert_eq!(sample.bytes_per_second, Some(0.0));
    assert!(sample.eta.is_none());
}
