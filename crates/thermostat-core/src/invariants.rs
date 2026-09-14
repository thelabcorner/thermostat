use thermostat_model::{Mode, OutputVector};

use crate::ControllerState;

/// A violation of a controller invariant that must never reach a physical
/// actuator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvariantViolation {
    HeatCoolConflict,
    UnarmedOutputEnergized,
    FaultedOutputEnergized,
    OffModeConditioningEnergized,
}

/// Checks invariants that depend only on a logical output vector.
///
/// # Errors
///
/// Returns [`InvariantViolation::HeatCoolConflict`] when heating and cooling
/// calls are requested simultaneously.
pub const fn validate_output_vector(outputs: OutputVector) -> Result<(), InvariantViolation> {
    if outputs.w && outputs.y {
        Err(InvariantViolation::HeatCoolConflict)
    } else {
        Ok(())
    }
}

/// Checks safety invariants observable from a complete controller state.
///
/// This function is intentionally side-effect free so it can be reused by
/// unit tests, fuzz targets, the simulator, and future actuator boundaries.
///
/// # Errors
///
/// Returns the first violated hard state/output invariant.
pub const fn validate_state(state: &ControllerState) -> Result<(), InvariantViolation> {
    if state.outputs.w && state.outputs.y {
        return Err(InvariantViolation::HeatCoolConflict);
    }
    if !state.actuator_armed && !state.outputs.is_all_off() {
        return Err(InvariantViolation::UnarmedOutputEnergized);
    }
    if state.critical_fault.is_some() && !state.outputs.is_all_off() {
        return Err(InvariantViolation::FaultedOutputEnergized);
    }
    if matches!(state.mode, Mode::Off) && (state.outputs.w || state.outputs.y) {
        return Err(InvariantViolation::OffModeConditioningEnergized);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use thermostat_model::OutputVector;

    use super::*;
    use crate::CriticalFault;

    #[test]
    fn conflicting_output_vector_is_rejected() {
        let output = OutputVector {
            w: true,
            y: true,
            g: false,
        };
        assert_eq!(
            validate_output_vector(output),
            Err(InvariantViolation::HeatCoolConflict)
        );
    }

    #[test]
    fn boot_state_satisfies_invariants() {
        assert_eq!(validate_state(&ControllerState::new(0)), Ok(()));
    }

    #[test]
    fn faulted_energized_state_is_rejected() {
        let mut state = ControllerState::new(0);
        state.actuator_armed = true;
        state.critical_fault = Some(CriticalFault::External);
        state.outputs.w = true;
        assert_eq!(
            validate_state(&state),
            Err(InvariantViolation::FaultedOutputEnergized)
        );
    }

    #[test]
    fn unarmed_energized_state_is_rejected() {
        let mut state = ControllerState::new(0);
        state.outputs.g = true;
        assert_eq!(
            validate_state(&state),
            Err(InvariantViolation::UnarmedOutputEnergized)
        );
    }

    #[test]
    fn off_mode_rejects_each_conditioning_contact_independently() {
        let mut state = ControllerState::new(0);
        state.actuator_armed = true;

        state.outputs.w = true;
        assert_eq!(
            validate_state(&state),
            Err(InvariantViolation::OffModeConditioningEnergized)
        );

        state.outputs = OutputVector {
            w: false,
            y: true,
            g: false,
        };
        assert_eq!(
            validate_state(&state),
            Err(InvariantViolation::OffModeConditioningEnergized)
        );
    }

    #[test]
    fn complete_state_rejects_simultaneous_heat_and_cool_first() {
        let mut state = ControllerState::new(0);
        state.actuator_armed = true;
        state.mode = Mode::Auto;
        state.outputs = OutputVector {
            w: true,
            y: true,
            g: true,
        };
        assert_eq!(
            validate_state(&state),
            Err(InvariantViolation::HeatCoolConflict)
        );
    }
}
