# Future Integration: ternary-locks

## Current State
Provides pattern-based lock algebra with Lock composition (AND/OR/NOT), lock compression, cross-model transfer scoring, critical mass detection, dependency graphs (LockGraph), and graveyard archaeology for expired locks.

## Integration Opportunities

### With ternary-room (Access Control)
Rooms need access control. A Lock represents a constraint that must be satisfied to enter a room. `LockComposition::And` requires multiple conditions (e.g., agent has skill X AND room has resource Y). `LockGraph` maps the dependency structure — which locks must be acquired before others. This is room access control as algebra.

### With ternary-protocol
Protocol messages carry authorization tokens. A `Lock` on a room checks whether the token pattern satisfies the lock. `Lock::satisfies()` is the gate function. Cross-model transfer scoring (`lock_transfer_score`) enables an agent authorized for one room to gain partial access to related rooms — the mathematical basis for role-based access control.

### With compiled-policy-c
The compiled policy C library deploys policies on microcontrollers. `ternary-locks` provides the policy specification language; `compiled-policy-c` compiles locks into zero-dependency C for ESP32 deployment. A `Lock` with `Pattern` becomes a bitmask check on a microcontroller.

## Potential in Mature Systems
In room-as-codespace, Lock is the authorization layer. Each room has a `LockGraph` specifying entry requirements. Agents carry pattern credentials. The graveyard tracks which locks have expired — rooms that no longer need access control, or agents whose credentials have lapsed. Critical mass detection identifies when a lock becomes so specific that only one agent can satisfy it.

## Cross-Pollination Ideas
- Lock compression for minimizing authorization metadata on bandwidth-constrained edges (ESP32)
- Graveyard archaeology as an audit trail — which rooms were accessible to whom, when
- Lock dependency graphs as room navigation planners — acquire locks in topological order

## Dependencies for Next Steps
- ternary-room needs a `RoomLock` trait wrapping `Lock`
- ternary-protocol message headers need pattern fields for lock checking
- compiled-policy-c needs a lock-to-bitmask compiler pass
