use core::fmt;

/// Fixed-point temperature in thousandths of one degree Celsius.
///
/// The control core uses integer temperature arithmetic so hysteresis boundary
/// behavior is deterministic and does not depend on floating-point rounding.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MilliCelsius(i32);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TemperatureError {
    NotFinite,
    OutOfRange,
}

impl MilliCelsius {
    /// Broad representable range used at generic ingestion boundaries.
    /// Installation-specific plausibility limits are intentionally narrower.
    pub const MIN: Self = Self(-100_000);
    pub const MAX: Self = Self(200_000);

    #[must_use]
    pub const fn from_milli_celsius(value: i32) -> Option<Self> {
        if value < Self::MIN.0 || value > Self::MAX.0 {
            None
        } else {
            Some(Self(value))
        }
    }

    /// Converts a Celsius floating-point value at an external ingestion boundary.
    ///
    /// # Errors
    ///
    /// Returns [`TemperatureError::NotFinite`] for NaN/infinity and
    /// [`TemperatureError::OutOfRange`] when the value exceeds the broad domain
    /// range represented by this type.
    pub fn from_celsius(value: f64) -> Result<Self, TemperatureError> {
        if !value.is_finite() {
            return Err(TemperatureError::NotFinite);
        }

        let milli = value * 1000.0;
        if milli < f64::from(Self::MIN.0) {
            return Err(TemperatureError::OutOfRange);
        }
        if milli > f64::from(Self::MAX.0) {
            return Err(TemperatureError::OutOfRange);
        }

        // The conversion boundary is the only place where a floating-point
        // temperature is accepted. Core comparisons remain integer-only.
        #[allow(clippy::cast_possible_truncation)]
        let rounded = milli.round() as i32;
        Self::from_milli_celsius(rounded).ok_or(TemperatureError::OutOfRange)
    }

    #[must_use]
    pub const fn as_milli_celsius(self) -> i32 {
        self.0
    }

    #[must_use]
    pub fn as_celsius(self) -> f64 {
        f64::from(self.0) / 1000.0
    }
}

impl fmt::Display for MilliCelsius {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.3} °C", self.as_celsius())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floating_input_is_rounded_once_at_boundary() {
        let value = MilliCelsius::from_celsius(21.0625);
        assert_eq!(value, Ok(MilliCelsius(21_063)));
    }

    #[test]
    fn rejects_non_finite_values() {
        assert_eq!(
            MilliCelsius::from_celsius(f64::NAN),
            Err(TemperatureError::NotFinite)
        );
        assert_eq!(
            MilliCelsius::from_celsius(f64::INFINITY),
            Err(TemperatureError::NotFinite)
        );
    }

    #[test]
    fn integer_constructor_accepts_closed_domain_and_rejects_neighbors() {
        assert_eq!(
            MilliCelsius::from_milli_celsius(MilliCelsius::MIN.as_milli_celsius()),
            Some(MilliCelsius::MIN)
        );
        assert_eq!(
            MilliCelsius::from_milli_celsius(MilliCelsius::MAX.as_milli_celsius()),
            Some(MilliCelsius::MAX)
        );
        assert_eq!(MilliCelsius::from_milli_celsius(-100_001), None);
        assert_eq!(MilliCelsius::from_milli_celsius(200_001), None);
    }

    #[test]
    fn floating_constructor_rejects_finite_values_outside_domain() {
        assert_eq!(
            MilliCelsius::from_celsius(-100.001),
            Err(TemperatureError::OutOfRange)
        );
        assert_eq!(
            MilliCelsius::from_celsius(200.001),
            Err(TemperatureError::OutOfRange)
        );
        assert_eq!(MilliCelsius::from_celsius(-100.0), Ok(MilliCelsius::MIN));
        assert_eq!(MilliCelsius::from_celsius(200.0), Ok(MilliCelsius::MAX));
    }

    #[test]
    fn accessors_and_display_are_stable() {
        let value = MilliCelsius::from_milli_celsius(21_063)
            .unwrap_or_else(|| panic!("test value must be in range"));
        assert_eq!(value.as_milli_celsius(), 21_063);
        assert!((value.as_celsius() - 21.063).abs() < f64::EPSILON);
        assert_eq!(value.to_string(), "21.063 °C");
    }
}
