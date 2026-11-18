//! 小さな共通ユーティリティ。

use std::time::{SystemTime, UNIX_EPOCH};

/// 現在時刻(UTC の Unix ミリ秒)。
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Unix ミリ秒 → UTC の (年, 月)。画像の保存先 `YYYY/MM/` に使う(外部クレート不要)。
/// 日付算出は Howard Hinnant の civil_from_days アルゴリズム。
pub fn year_month(unix_ms: i64) -> (i32, u32) {
    let days = unix_ms.div_euclid(86_400_000);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year as i32, month as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn year_month_known_dates() {
        assert_eq!(year_month(0), (1970, 1));
        assert_eq!(year_month(951_782_400_000), (2000, 2)); // 2000-02-29
        assert_eq!(year_month(1_709_251_199_000), (2024, 2)); // 2024-02-29 23:59:59
        assert_eq!(year_month(1_709_251_200_000), (2024, 3)); // 2024-03-01 00:00:00
        assert_eq!(year_month(1_798_761_599_000), (2026, 12)); // 2026-12-31 23:59:59
        assert_eq!(year_month(1_798_761_600_000), (2027, 1)); // 2027-01-01
        assert_eq!(year_month(-1), (1969, 12));
    }

    #[test]
    fn now_is_after_2025() {
        assert!(now_ms() > 1_735_689_600_000);
    }
}
