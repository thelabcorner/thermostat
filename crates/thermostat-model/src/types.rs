#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum Mode {
    #[default]
    Off,
    Heat,
    Cool,
    Auto,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum FanMode {
    #[default]
    Auto,
    On,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum Demand {
    #[default]
    None,
    Heat,
    Cool,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum EquipmentAction {
    #[default]
    Idle,
    Heat,
    Cool,
    FanOnly,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EquipmentFamily {
    Heat,
    Cool,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum BlockReason {
    #[default]
    None,
    StartupLockout,
    MinHeatOff,
    MinHeatRun,
    MinCoolOff,
    MinCoolRun,
    ChangeoverDelay,
    SensorInvalid,
    ActuatorNotArmed,
    ConfigInvalid,
    CriticalFault,
    NonMonotonicTime,
}

/// Logical thermostat contacts, not GPIO levels.
///
/// `true` means the corresponding thermostat call is requested. Active-low
/// relay modules are handled only by the future actuator implementation.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct OutputVector {
    pub w: bool,
    pub y: bool,
    pub g: bool,
}

impl OutputVector {
    pub const OFF: Self = Self {
        w: false,
        y: false,
        g: false,
    };

    #[must_use]
    pub const fn is_all_off(self) -> bool {
        !self.w && !self.y && !self.g
    }
}
