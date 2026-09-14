---------------------------- MODULE ThermostatSafety ----------------------------
EXTENDS Integers, Naturals, TLC

CONSTANTS CoolLockoutTicks, ChangeoverTicks

Actions == {"Idle", "Heat", "Cool"}

VARIABLES armed, faulted, action, previousAction, coolLockout, changeover

vars == <<armed, faulted, action, previousAction, coolLockout, changeover>>

OutputW == action = "Heat"
OutputY == action = "Cool"

Init ==
    /\ armed = FALSE
    /\ faulted = FALSE
    /\ action = "Idle"
    /\ previousAction = "Idle"
    /\ coolLockout = CoolLockoutTicks
    /\ changeover = 0

RememberPrevious == previousAction' = action

Tick ==
    /\ RememberPrevious
    /\ coolLockout' = IF coolLockout > 0 THEN coolLockout - 1 ELSE 0
    /\ changeover' = IF changeover > 0 THEN changeover - 1 ELSE 0
    /\ UNCHANGED <<armed, faulted, action>>

Arm ==
    /\ ~armed
    /\ RememberPrevious
    /\ armed' = TRUE
    /\ UNCHANGED <<faulted, action, coolLockout, changeover>>

Disarm ==
    /\ armed
    /\ RememberPrevious
    /\ armed' = FALSE
    /\ action' = "Idle"
    /\ UNCHANGED <<faulted, coolLockout, changeover>>

RaiseFault ==
    /\ ~faulted
    /\ RememberPrevious
    /\ faulted' = TRUE
    /\ action' = "Idle"
    /\ UNCHANGED <<armed, coolLockout, changeover>>

ClearFault ==
    /\ faulted
    /\ RememberPrevious
    /\ faulted' = FALSE
    /\ UNCHANGED <<armed, action, coolLockout, changeover>>

StartHeat ==
    /\ armed
    /\ ~faulted
    /\ action = "Idle"
    /\ changeover = 0
    /\ RememberPrevious
    /\ action' = "Heat"
    /\ UNCHANGED <<armed, faulted, coolLockout, changeover>>

StopHeat ==
    /\ action = "Heat"
    /\ RememberPrevious
    /\ action' = "Idle"
    /\ changeover' = ChangeoverTicks
    /\ UNCHANGED <<armed, faulted, coolLockout>>

StartCool ==
    /\ armed
    /\ ~faulted
    /\ action = "Idle"
    /\ coolLockout = 0
    /\ changeover = 0
    /\ RememberPrevious
    /\ action' = "Cool"
    /\ UNCHANGED <<armed, faulted, coolLockout, changeover>>

StopCool ==
    /\ action = "Cool"
    /\ RememberPrevious
    /\ action' = "Idle"
    /\ coolLockout' = CoolLockoutTicks
    /\ changeover' = ChangeoverTicks
    /\ UNCHANGED <<armed, faulted>>

Next ==
    \/ Tick
    \/ Arm
    \/ Disarm
    \/ RaiseFault
    \/ ClearFault
    \/ StartHeat
    \/ StopHeat
    \/ StartCool
    \/ StopCool

Spec == Init /\ [][Next]_vars

TypeOK ==
    /\ armed \in BOOLEAN
    /\ faulted \in BOOLEAN
    /\ action \in Actions
    /\ previousAction \in Actions
    /\ coolLockout \in 0..CoolLockoutTicks
    /\ changeover \in 0..ChangeoverTicks

MutualExclusion == ~(OutputW /\ OutputY)

UnarmedSafe == ~armed => action = "Idle"

FaultSafe == faulted => action = "Idle"

CoolingLockoutSafe == action = "Cool" => (coolLockout = 0 /\ changeover = 0)

NoDirectFamilyFlip ==
    ~((previousAction = "Heat" /\ action = "Cool")
      \/ (previousAction = "Cool" /\ action = "Heat"))

=============================================================================
