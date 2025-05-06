use chrono::Utc;

/// Returns a string representing the elapsed time since `start_time` like
/// " in 1m15s" or " in 1.50s".
pub fn format_elapsed(start_time: chrono::DateTime<Utc>) -> String {
    let elapsed = Utc::now().signed_duration_since(start_time);
    format_duration(elapsed)
}

fn format_duration(elapsed: chrono::TimeDelta) -> String {
    let millis = elapsed.num_milliseconds();
    if millis < 1000 {
        format!(" in {}ms", millis)
    } else if millis < 60_000 {
        format!(" in {:.2}s", millis as f64 / 1000.0)
    } else {
        let minutes = millis / 60_000;
        let seconds = (millis % 60_000) / 1000;
        format!(" in {minutes}m{seconds:.2}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn test_format_duration_subsecond() {
        // Durations < 1s should be rendered in milliseconds with no decimals.
        let dur = Duration::milliseconds(250);
        assert_eq!(format_duration(dur), " in 250ms");

        // Exactly zero should still work.
        let dur_zero = Duration::milliseconds(0);
        assert_eq!(format_duration(dur_zero), " in 0ms");
    }

    #[test]
    fn test_format_duration_seconds() {
        // Durations between 1s (inclusive) and 60s (exclusive) should be
        // printed with 2-decimal-place seconds.
        let dur = Duration::milliseconds(1_500); // 1.5s
        assert_eq!(format_duration(dur), " in 1.50s");

        // 59.999s rounds to 60.00s
        let dur2 = Duration::milliseconds(59_999);
        assert_eq!(format_duration(dur2), " in 60.00s");
    }

    #[test]
    fn test_format_duration_minutes() {
        // Durations ≥ 1 minute should be printed mmss.
        let dur = Duration::milliseconds(75_000); // 1m15s
        assert_eq!(format_duration(dur), " in 1m15s");

        let dur_exact = Duration::milliseconds(60_000); // 1m0s
        assert_eq!(format_duration(dur_exact), " in 1m0s");

        let dur_long = Duration::milliseconds(3_601_000);
        assert_eq!(format_duration(dur_long), " in 60m1s");
    }
}
