//! Verbatim port of `@graphql-eslint`'s `require-deprecation-date` test suite.
//!
//! Upstream:
//!   <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-deprecation-date/index.test.ts>

use super::harness::{Case, ExpectedError};
use crate::rules::require_deprecation_date::RequireDeprecationDateRuleImpl;

/// Compute tomorrow's date as `"DD/MM/YYYY"`, matching upstream's
/// `const tomorrow = ...` computation. Used by the valid cases that
/// pass a deletion date one day in the future.
fn tomorrow_date_string() -> String {
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Add one day in seconds
    let tomorrow_secs = now_secs + 86_400;
    // Compute year/month/day from Unix timestamp (UTC)
    let days = tomorrow_secs / 86_400;
    let (year, month, day) = days_to_ymd(days);
    format!("{day:02}/{month:02}/{year}")
}

/// Convert days-since-epoch to (year, month, day) in UTC.
// Hinnant's civil_from_days uses signed/unsigned casts that are correct within
// any plausible date range; allow the cast lints rather than obscure the algorithm.
#[allow(
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
)]
fn days_to_ymd(days: u64) -> (u32, u32, u32) {
    // Algorithm: Howard Hinnant's civil_from_days, adapted for u64 input.
    let z = days as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = i64::from(yoe) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as u32, m, d)
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-deprecation-date/index.test.ts#L16>
#[test]
fn valid_l16_no_deprecated() {
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-deprecation-date/index.test.ts#L16",
        super::UPSTREAM_SHA,
    ))
    .code("type User { firstName: String }")
    .run_against_standalone_schema(RequireDeprecationDateRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-deprecation-date/index.test.ts#L17>
#[test]
fn valid_l17_deprecated_with_tomorrow_deletion_date() {
    // Tomorrow is always in the future so the rule should not fire.
    let tomorrow = tomorrow_date_string();
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-deprecation-date/index.test.ts#L17",
        super::UPSTREAM_SHA,
    ))
    .code(format!(
        r#"scalar Old @deprecated(deletionDate: "{tomorrow}")"#,
    ))
    .run_against_standalone_schema(RequireDeprecationDateRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-deprecation-date/index.test.ts#L18>
#[test]
fn valid_l18_custom_argument_name_with_tomorrow() {
    let tomorrow = tomorrow_date_string();
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-deprecation-date/index.test.ts#L18",
        super::UPSTREAM_SHA,
    ))
    .code(format!(
        r#"scalar Old @deprecated(untilDate: "{tomorrow}")"#,
    ))
    .options(serde_json::json!({ "argumentName": "untilDate" }))
    .run_against_standalone_schema(RequireDeprecationDateRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-deprecation-date/index.test.ts#L22>
#[test]
fn valid_l22_field_with_future_deletion_date() {
    // `firstname` has a far-future deletionDate → no error. `firstName` has no
    // `@deprecated` at all → also no error.
    Case::valid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-deprecation-date/index.test.ts#L22",
        super::UPSTREAM_SHA,
    ))
    .code(
        r#"
        type User {
          firstname: String @deprecated(deletionDate: "22/08/2031")
          firstName: String
        }
      "#,
    )
    .run_against_standalone_schema(RequireDeprecationDateRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-deprecation-date/index.test.ts#L32>
#[test]
fn invalid_l32_past_deletion_date() {
    // "22/08/2021" is in the past → MESSAGE_CAN_BE_REMOVED.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-deprecation-date/index.test.ts#L32",
        super::UPSTREAM_SHA,
    ))
    .code(r#"scalar Old @deprecated(deletionDate: "22/08/2021")"#)
    .errors(vec![ExpectedError::new()])
    .run_against_standalone_schema(RequireDeprecationDateRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-deprecation-date/index.test.ts#L36>
#[test]
fn invalid_l36_past_deletion_date_custom_arg() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-deprecation-date/index.test.ts#L36",
        super::UPSTREAM_SHA,
    ))
    .code(r#"scalar Old @deprecated(untilDate: "22/08/2021")"#)
    .options(serde_json::json!({ "argumentName": "untilDate" }))
    .errors(vec![ExpectedError::new()])
    .run_against_standalone_schema(RequireDeprecationDateRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-deprecation-date/index.test.ts#L40>
#[test]
fn invalid_l40_bad_date_format() {
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-deprecation-date/index.test.ts#L40",
        super::UPSTREAM_SHA,
    ))
    .code(r#"scalar Old @deprecated(deletionDate: "bad")"#)
    .errors(vec![ExpectedError::new()])
    .run_against_standalone_schema(RequireDeprecationDateRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-deprecation-date/index.test.ts#L44>
#[test]
fn invalid_l44_invalid_calendar_date() {
    // "32/08/2021" matches the format regex but day 32 is impossible.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-deprecation-date/index.test.ts#L44",
        super::UPSTREAM_SHA,
    ))
    .code(r#"scalar Old @deprecated(deletionDate: "32/08/2021")"#)
    .errors(vec![ExpectedError::new()])
    .run_against_standalone_schema(RequireDeprecationDateRuleImpl);
}

/// <https://github.com/dimaMachina/graphql-eslint/blob/f0f200ef0b030cb8a905bbcb32fe346b87cc2e24/packages/plugin/src/rules/require-deprecation-date/index.test.ts#L48>
#[test]
fn invalid_l48_no_deletion_date_argument() {
    // `@deprecated` without a `deletionDate` argument → MESSAGE_REQUIRE_DATE.
    Case::invalid(format!(
        "https://github.com/dimaMachina/graphql-eslint/blob/{}/packages/plugin/src/rules/require-deprecation-date/index.test.ts#L48",
        super::UPSTREAM_SHA,
    ))
    .code("type Old { oldField: ID @deprecated }")
    .errors(vec![ExpectedError::new()])
    .run_against_standalone_schema(RequireDeprecationDateRuleImpl);
}
