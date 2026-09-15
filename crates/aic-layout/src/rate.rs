//! Exact rates shared by physical layouts and production throughput.
use serde::{Deserialize, Serialize};
const STAGE: &str = "recipe-throughput";

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
pub struct Rate {
    pub numerator: i64,
    pub denominator: i64,
}

impl Rate {
    pub fn zero() -> Self {
        Self {
            numerator: 0,
            denominator: 1,
        }
    }

    pub fn from_quantity_per_duration_ms(
        quantity: i64,
        duration_ms: i64,
    ) -> Result<Self, ThroughputDiagnostic> {
        if quantity <= 0 {
            return Err(ThroughputDiagnostic::error(
                "non-positive-target-quantity",
                "/target/quantity",
                None,
                format!("target quantity must be positive, found {quantity}"),
            ));
        }

        if duration_ms <= 0 {
            return Err(ThroughputDiagnostic::error(
                "non-positive-target-duration",
                "/target/duration_ms",
                None,
                format!("target duration_ms must be positive, found {duration_ms}"),
            ));
        }

        let numerator = checked_mul_i128(quantity as i128, 1000, "rate-normalization")?;
        Self::normalize(numerator, duration_ms as i128, "rate-normalization")
    }

    pub fn checked_add(self, other: Self) -> Result<Self, ThroughputDiagnostic> {
        let left = checked_mul_i128(
            self.numerator as i128,
            other.denominator as i128,
            "rate-addition",
        )?;
        let right = checked_mul_i128(
            other.numerator as i128,
            self.denominator as i128,
            "rate-addition",
        )?;
        let numerator = checked_add_i128(left, right, "rate-addition")?;
        let denominator = checked_mul_i128(
            self.denominator as i128,
            other.denominator as i128,
            "rate-addition",
        )?;

        Self::normalize(numerator, denominator, "rate-addition")
    }

    pub fn checked_sub(self, other: Self) -> Result<Self, ThroughputDiagnostic> {
        let left = checked_mul_i128(
            self.numerator as i128,
            other.denominator as i128,
            "rate-subtraction",
        )?;
        let right = checked_mul_i128(
            other.numerator as i128,
            self.denominator as i128,
            "rate-subtraction",
        )?;
        let numerator = checked_sub_i128(left, right, "rate-subtraction")?;
        let denominator = checked_mul_i128(
            self.denominator as i128,
            other.denominator as i128,
            "rate-subtraction",
        )?;

        Self::normalize(numerator, denominator, "rate-subtraction")
    }

    pub fn checked_mul_i64(self, rhs: i64) -> Result<Self, ThroughputDiagnostic> {
        if rhs < 0 {
            return Err(arithmetic_overflow("rate-multiplication"));
        }

        let numerator =
            checked_mul_i128(self.numerator as i128, rhs as i128, "rate-multiplication")?;
        Self::normalize(numerator, self.denominator as i128, "rate-multiplication")
    }

    pub fn checked_div_i64(self, rhs: i64) -> Result<Self, ThroughputDiagnostic> {
        if rhs <= 0 {
            return Err(arithmetic_overflow("rate-division"));
        }

        let denominator = checked_mul_i128(self.denominator as i128, rhs as i128, "rate-division")?;
        Self::normalize(self.numerator as i128, denominator, "rate-division")
    }

    pub fn checked_work_seconds_per_second(
        self,
        duration_ms: i64,
    ) -> Result<Self, ThroughputDiagnostic> {
        if duration_ms <= 0 {
            return Err(arithmetic_overflow("work-rate-calculation"));
        }

        let numerator = checked_mul_i128(
            self.numerator as i128,
            duration_ms as i128,
            "work-rate-calculation",
        )?;
        let denominator =
            checked_mul_i128(self.denominator as i128, 1000, "work-rate-calculation")?;
        Self::normalize(numerator, denominator, "work-rate-calculation")
    }

    pub fn max(self, other: Self) -> Self {
        if self >= other { self } else { other }
    }

    pub fn is_zero(self) -> bool {
        self.numerator == 0
    }

    pub fn normalize(
        numerator: i128,
        denominator: i128,
        operation: &'static str,
    ) -> Result<Self, ThroughputDiagnostic> {
        if numerator < 0 || denominator <= 0 {
            return Err(arithmetic_overflow(operation));
        }

        if numerator == 0 {
            return Ok(Self::zero());
        }

        let divisor = gcd(numerator, denominator);
        let numerator = numerator / divisor;
        let denominator = denominator / divisor;

        if numerator > i64::MAX as i128 || denominator > i64::MAX as i128 {
            return Err(arithmetic_overflow(operation));
        }

        Ok(Self {
            numerator: numerator as i64,
            denominator: denominator as i64,
        })
    }
}

impl PartialOrd for Rate {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Rate {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let left = (self.numerator as i128) * (other.denominator as i128);
        let right = (other.numerator as i128) * (self.denominator as i128);
        left.cmp(&right)
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ThroughputDiagnostic {
    pub stage: &'static str,
    pub severity: &'static str,
    pub code: &'static str,
    pub path: String,
    pub entity: Option<String>,
    pub message: String,
}

impl ThroughputDiagnostic {
    pub fn error(
        code: &'static str,
        path: impl Into<String>,
        entity: Option<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            stage: STAGE,
            severity: "error",
            code,
            path: path.into(),
            entity,
            message: message.into(),
        }
    }

    pub fn info(
        code: &'static str,
        path: impl Into<String>,
        entity: Option<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            stage: STAGE,
            severity: "info",
            code,
            path: path.into(),
            entity,
            message: message.into(),
        }
    }
}

fn checked_add_i128(
    left: i128,
    right: i128,
    operation: &'static str,
) -> Result<i128, ThroughputDiagnostic> {
    left.checked_add(right)
        .ok_or_else(|| arithmetic_overflow(operation))
}

fn checked_sub_i128(
    left: i128,
    right: i128,
    operation: &'static str,
) -> Result<i128, ThroughputDiagnostic> {
    left.checked_sub(right)
        .ok_or_else(|| arithmetic_overflow(operation))
}

fn checked_mul_i128(
    left: i128,
    right: i128,
    operation: &'static str,
) -> Result<i128, ThroughputDiagnostic> {
    left.checked_mul(right)
        .ok_or_else(|| arithmetic_overflow(operation))
}

fn arithmetic_overflow(operation: &'static str) -> ThroughputDiagnostic {
    ThroughputDiagnostic::error(
        "arithmetic-overflow",
        "/",
        None,
        format!("arithmetic overflow while performing {operation}"),
    )
}

fn gcd(mut left: i128, mut right: i128) -> i128 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }

    left
}
