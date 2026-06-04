# ternary-locks

Lock algebra for ternary pattern abstraction — constraint composition, compression, dependency graphs, critical mass detection, and graveyard archaeology for expired patterns.

## Why This Exists

In complex systems, **patterns** are how we recognize structure. But raw patterns are noisy and redundant. This crate provides a mathematical framework for abstracting ternary patterns {-1, 0, +1} into **locks** — composable constraints that can be combined (AND/OR/NOT), compressed by merging overlapping coverage, tracked through dependency graphs, and analyzed even after expiration.

The name comes from Oracle1's research into "lock" as a pattern abstraction: a lock is a constraint that must be satisfied, where 0 acts as a wildcard (don't care). Locks compose algebraically — you can AND two locks for intersection, OR for union, NOT for negation. This creates a rich algebra for reasoning about ternary pattern spaces.

The **graveyard** module tracks expired locks and enables "archaeology" — analyzing which patterns were strongest when they died, which could be revived, and computing the half-life of the lock population. This is pattern-level memory for evolving systems.

This crate is part of the **Negative Space Intelligence** ecosystem.

## Core Concepts

- **Lock** — A ternary pattern with an ID, strength, and active flag. Zeros are wildcards; non-zero positions are constraints. Supports satisfaction checking and specificity comparison.
- **LockComposition** — Algebraic composition of locks via AND, OR, NOT. Evaluates against a set of satisfied lock IDs. Supports tree depth and referenced ID extraction.
- **Lock Compression** — Merges overlapping locks by filling in wildcards from compatible patterns, reducing redundancy while preserving coverage.
- **LockGraph** — Dependency graph with topological ordering, cycle detection, and transitive dependent lookup.
- **Graveyard** — Cemetery for expired locks with strength analytics, pattern frequency analysis, revival detection, and half-life computation.
- **Transfer Score** — Measures how well patterns from one domain transfer to another.
- **Critical Mass Detection** — Determines when enough locks cover a pattern space sufficiently.

## Quick Start

```toml
# Cargo.toml
[dependencies]
ternary-locks = "0.1"
```

```rust
use ternary_locks::*;
use std::collections::HashSet;

// Create locks with ternary patterns (0 = wildcard)
let lock_a = Lock::new("pattern_a", vec![1, 0, -1]);   // match +1, any, -1
let lock_b = Lock::new("pattern_b", vec![1, -1, 0]);   // match +1, -1, any

// Check satisfaction
assert!(lock_a.satisfies(&vec![1, 1, -1]));  // wildcard at pos 1
assert!(!lock_a.satisfies(&vec![-1, 0, -1])); // pos 0 mismatch

// Compose locks with AND/OR/NOT
let composition = LockComposition::and(
    LockComposition::single("pattern_a"),
    LockComposition::or(
        LockComposition::single("pattern_b"),
        LockComposition::single("pattern_c"),
    ),
);
let mut satisfied: HashSet<&str> = HashSet::new();
satisfied.insert("pattern_a");
satisfied.insert("pattern_b");
assert!(composition.evaluate(&satisfied));

// Compress overlapping locks
let locks = vec![
    Lock::new("a", vec![1, 0, 0]),
    Lock::new("b", vec![0, -1, 0]),
    Lock::new("c", vec![1, -1, 0]),  // merges with both a and b
];
let compressed = compress_locks(&locks);

// Dependency graph with cycle detection
let mut graph = LockGraph::new();
graph.add_lock(Lock::new("base", vec![1]));
graph.add_lock(Lock::new("derived", vec![-1]));
graph.add_dependency("derived", "base");
assert!(!graph.has_cycles());

// Graveyard analytics
let mut gy = Graveyard::new();
gy.bury(Lock::new("old_a", vec![1, -1]).with_strength(5), 10);
gy.bury(Lock::new("old_b", vec![0, 1]).with_strength(8), 20);
println!("Average strength: {:.2}", gy.avg_strength());
println!("Half-life at step: {:?}", gy.half_life());

// Find revivable locks
let active = vec![Lock::new("current", vec![0, -1, 0])];
let revivable = gy.revivable(&active);
```

## API Overview

### Lock
| Method | Description |
|---|---|
| `new(id, pattern)` | Create a lock with ternary pattern (0 = wildcard) |
| `with_strength(s)` | Set constraint strength |
| `satisfies(input)` | Check if input matches the pattern |
| `is_more_specific_than(other)` | Fewer wildcards = more specific |
| `expire()` | Deactivate |

### LockComposition
| Variant | Description |
|---|---|
| `Single(id)` | Leaf node referencing one lock |
| `And(l, r)` | Both must be satisfied |
| `Or(l, r)` | Either must be satisfied |
| `Not(inner)` | Negation |

### Functions
| Function | Description |
|---|---|
| `compress_locks(locks)` | Merge overlapping patterns |
| `transfer_score(source, target)` | Cross-domain pattern similarity (0.0–1.0) |
| `detect_critical_mass(locks, len, threshold)` | Check pattern space coverage |

### LockGraph
| Method | Description |
|---|---|
| `add_lock(lock)` | Register a lock |
| `add_dependency(from, to)` | Create dependency edge |
| `topological_order()` | Kahn's algorithm ordering |
| `dependents(id)` | All transitive dependents |
| `has_cycles()` | Cycle detection via topo sort |

### Graveyard
| Method | Description |
|---|---|
| `bury(lock, step)` | Inter an expired lock |
| `avg_strength()` | Mean strength of buried locks |
| `strongest(n)` | Top N by strength |
| `pattern_frequency()` | Histogram of pattern values |
| `revivable(active)` | Buried locks overlapping with active ones |
| `half_life()` | Median burial step |

## How It Works

Locks use a ternary pattern space where 0 serves as a "don't care" wildcard. A lock `[1, 0, -1]` matches any input where the first element is +1 and the third is -1, regardless of the second element. This creates a compact representation: instead of enumerating all matching inputs, you specify constraints at specific positions.

Compression works by finding pairs of locks with compatible patterns (no conflicting non-zero values at the same position) and merging them into a single lock that combines the constraints of both. The merged lock inherits the union of all non-zero positions, producing a more specific but still correct constraint.

The graveyard enables **post-mortem analysis**: tracking which patterns were strong when they expired, computing how quickly the population turns over (half-life), and detecting "revivable" locks whose patterns overlap with currently active ones — patterns that may have been prematurely discarded.

## Use Cases

1. **Pattern mining** — Extract recurring ternary patterns from data streams, compress them into minimal lock sets, and detect when enough patterns have been found to cover the interesting space.

2. **Cross-domain transfer** — When building ternary models across domains (e.g., market signals → cellular states), use `transfer_score` to quantify how well learned patterns transfer.

3. **Dependency tracking** — Model cascading constraints where one pattern's satisfaction enables another. The `LockGraph` provides topological ordering and cycle detection for safe evaluation.

4. **Evolving systems** — Track pattern birth and death over time. The graveyard's half-life and strength analytics reveal whether the system is converging (long-lived, strong locks) or churning (rapid turnover).

## Ecosystem

| Crate | Relationship |
|---|---|
| `ternary-cell` | Cell state patterns can be abstracted into locks |
| `ternary-network` | Network structure can be encoded as lock constraints |
| `ternary-logic` | Lock composition uses ternary logic for AND/OR/NOT |
| `ternary-bayesian` | Locks define patterns; Bayesian inference reasons about them |
| `ternary-attention` | Attention patterns can be captured as lock constraints |

## Known Limitations

- **`compress_locks` is greedy O(n²).** The compression algorithm merges overlapping locks greedily and does not find the optimal minimal set.
- **`transfer_score` is ad-hoc.** The formula counts pairs with any non-zero agreement, weighting a single matching position the same as full agreement. It is not a rigorous cross-domain similarity metric.
- **`detect_critical_mass` uses a heuristic.** For pattern spaces larger than ~8 positions, the coverage estimate `locks * 3 / length` has no theoretical basis.
- **`Graveyard::half_life` computes median burial step**, not a true statistical half-life (exponential decay rate).
- **No cycle detection in LockGraph evaluation.** While `has_cycles()` exists, applying locks with circular dependencies will silently produce incorrect results.

## License

MIT
