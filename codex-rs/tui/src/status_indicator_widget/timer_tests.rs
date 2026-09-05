use super::*;
use pretty_assertions::assert_eq;

#[test]
fn pauses_and_resumes_without_counting_wait_time() {
    let baseline = Instant::now();
    let mut timer = StatusTimer {
        last_resume_at: baseline,
        ..StatusTimer::default()
    };
    timer.pause_at(baseline + Duration::from_secs(5));
    timer.pause_at(baseline + Duration::from_secs(8));
    assert_eq!(
        timer.elapsed_at(baseline + Duration::from_secs(10)),
        Duration::from_secs(5)
    );
    timer.resume_at(baseline + Duration::from_secs(10));
    timer.resume_at(baseline + Duration::from_secs(12));
    assert_eq!(
        timer.elapsed_at(baseline + Duration::from_secs(13)),
        Duration::from_secs(8)
    );
    timer.reset(Duration::from_secs(24));
    assert_eq!(
        timer.elapsed_at(timer.last_resume_at),
        Duration::from_secs(24)
    );
    timer.pause_at(timer.last_resume_at);
    timer.reset(Duration::ZERO);
    assert_eq!(
        timer.elapsed_at(timer.last_resume_at + Duration::from_secs(/*secs*/ 120)),
        Duration::ZERO
    );
}
