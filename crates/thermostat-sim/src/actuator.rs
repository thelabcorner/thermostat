use thermostat_core::{InvariantViolation, OutputVector, validate_output_vector};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActuatorTransition {
    pub at_ms: u64,
    pub previous: OutputVector,
    pub next: OutputVector,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActuatorError {
    Invariant(InvariantViolation),
    NonMonotonicApplication,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VirtualActuator {
    current: OutputVector,
    last_applied_at_ms: Option<u64>,
    applications: u64,
    transitions: Vec<ActuatorTransition>,
}

impl VirtualActuator {
    #[must_use]
    pub const fn current(&self) -> OutputVector {
        self.current
    }

    #[must_use]
    pub const fn applications(&self) -> u64 {
        self.applications
    }

    #[must_use]
    pub fn transitions(&self) -> &[ActuatorTransition] {
        &self.transitions
    }

    /// Applies a complete logical output vector as one simulated transaction.
    ///
    /// # Errors
    ///
    /// Rejects an unsafe vector or an application timestamp older than the
    /// most recent transaction.
    pub fn apply(&mut self, at_ms: u64, next: OutputVector) -> Result<(), ActuatorError> {
        validate_output_vector(next).map_err(ActuatorError::Invariant)?;
        if self
            .last_applied_at_ms
            .is_some_and(|previous| at_ms < previous)
        {
            return Err(ActuatorError::NonMonotonicApplication);
        }

        self.applications = self.applications.saturating_add(1);
        self.last_applied_at_ms = Some(at_ms);
        if next != self.current {
            self.transitions.push(ActuatorTransition {
                at_ms,
                previous: self.current,
                next,
            });
            self.current = next;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_heat_cool_conflict() {
        let mut actuator = VirtualActuator::default();
        let result = actuator.apply(
            0,
            OutputVector {
                w: true,
                y: true,
                g: false,
            },
        );
        assert_eq!(
            result,
            Err(ActuatorError::Invariant(
                InvariantViolation::HeatCoolConflict
            ))
        );
        assert_eq!(actuator.current(), OutputVector::OFF);
    }

    #[test]
    fn records_only_output_changes() {
        let mut actuator = VirtualActuator::default();
        assert_eq!(actuator.apply(1, OutputVector::OFF), Ok(()));
        assert_eq!(actuator.apply(2, OutputVector::OFF), Ok(()));
        assert_eq!(actuator.transitions().len(), 0);
        assert_eq!(actuator.applications(), 2);
    }

    #[test]
    fn rejects_non_monotonic_application_without_mutating_actuator() {
        let mut actuator = VirtualActuator::default();
        let fan = OutputVector {
            w: false,
            y: false,
            g: true,
        };
        assert_eq!(actuator.apply(10, fan), Ok(()));
        let applications = actuator.applications();
        let transitions = actuator.transitions().len();

        assert_eq!(
            actuator.apply(9, OutputVector::OFF),
            Err(ActuatorError::NonMonotonicApplication)
        );
        assert_eq!(actuator.current(), fan);
        assert_eq!(actuator.applications(), applications);
        assert_eq!(actuator.transitions().len(), transitions);
    }
}
