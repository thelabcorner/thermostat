#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClockError {
    Overflow,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct VirtualClock {
    now_ms: u64,
}

impl VirtualClock {
    #[must_use]
    pub const fn new(now_ms: u64) -> Self {
        Self { now_ms }
    }

    #[must_use]
    pub const fn now_ms(self) -> u64 {
        self.now_ms
    }

    /// Advances virtual monotonic time.
    ///
    /// # Errors
    ///
    /// Returns [`ClockError::Overflow`] if the requested advance would exceed
    /// the representable monotonic timestamp range.
    pub fn advance_ms(&mut self, delta_ms: u64) -> Result<u64, ClockError> {
        self.now_ms = self
            .now_ms
            .checked_add(delta_ms)
            .ok_or(ClockError::Overflow)?;
        Ok(self.now_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_is_monotonic_and_checked() {
        let mut clock = VirtualClock::new(10);
        assert_eq!(clock.advance_ms(5), Ok(15));
        let mut maxed = VirtualClock::new(u64::MAX);
        assert_eq!(maxed.advance_ms(1), Err(ClockError::Overflow));
    }
}
