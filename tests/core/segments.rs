use super::*;

#[test]
fn segment_plans_cover_every_byte_without_overlap_or_unbounded_state() {
    for size in [
        0,
        1,
        3,
        16,
        131071,
        262144,
        262145,
        2 * 1024 * 1024,
        u64::MAX,
    ] {
        for connections in [1, 2, 4, 16] {
            let segments = Segment::plan(size, connections);
            assert!(segments.len() <= MAX_SEGMENTS);
            let mut next = 0;
            for segment in &segments {
                assert_eq!(segment.start, next);
                assert!(segment.end > segment.start);
                next = segment.end;
            }
            assert_eq!(next, size);
            if size > 0 {
                let smallest = segments.iter().map(|s| s.end - s.start).min().unwrap();
                let largest = segments.iter().map(|s| s.end - s.start).max().unwrap();
                assert!(largest - smallest <= 1);
            }
        }
    }
}

#[test]
fn large_downloads_offer_extra_work_and_small_files_keep_connection_sized_plans() {
    assert_eq!(Segment::plan(131071, 4).len(), 4);
    assert_eq!(Segment::plan(2 * 1024 * 1024, 2).len(), 8);
    assert_eq!(Segment::plan(u64::MAX, 16).len(), MAX_SEGMENTS);
}
