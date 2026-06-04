#![forbid(unsafe_code)]

//! Lock algebra inspired by Oracle1's research.
//!
//! Provides pattern abstraction via Lock, lock composition (AND/OR/NOT),
//! lock compression, cross-model transfer scoring, critical mass detection,
//! dependency graphs (LockGraph), and grave-yard archaeology (expired lock analysis).

use std::collections::{HashMap, HashSet, VecDeque};

/// A ternary pattern element.
pub type Pattern = Vec<i8>;

/// A lock represents a pattern abstraction — a constraint that must be satisfied.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Lock {
    pub id: String,
    pub pattern: Pattern,
    pub strength: u32,
    pub active: bool,
}

impl Lock {
    pub fn new(id: impl Into<String>, pattern: Pattern) -> Self {
        Lock {
            id: id.into(),
            pattern,
            strength: 1,
            active: true,
        }
    }

    pub fn with_strength(mut self, s: u32) -> Self {
        self.strength = s;
        self
    }

    /// Check if an input pattern satisfies this lock.
    pub fn satisfies(&self, input: &Pattern) -> bool {
        if input.len() != self.pattern.len() { return false; }
        self.pattern.iter().zip(input.iter()).all(|(a, b)| {
            // Lock is satisfied where pattern is 0 (wildcard), or matches exactly
            *a == 0 || *a == *b
        })
    }

    /// Check if this lock is more specific than another (fewer wildcards, same or more constraints).
    pub fn is_more_specific_than(&self, other: &Lock) -> bool {
        if self.pattern.len() != other.pattern.len() { return false; }
        let self_constraints: usize = self.pattern.iter().filter(|&&x| x != 0).count();
        let other_constraints: usize = other.pattern.iter().filter(|&&x| x != 0).count();
        self_constraints >= other_constraints
    }

    /// Deactivate this lock.
    pub fn expire(&mut self) {
        self.active = false;
    }
}

/// Composition of locks via AND, OR, NOT.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum LockComposition {
    Single(String),
    And(Box<LockComposition>, Box<LockComposition>),
    Or(Box<LockComposition>, Box<LockComposition>),
    Not(Box<LockComposition>),
}

impl LockComposition {
    pub fn single(id: impl Into<String>) -> Self {
        LockComposition::Single(id.into())
    }

    pub fn and(left: LockComposition, right: LockComposition) -> Self {
        LockComposition::And(Box::new(left), Box::new(right))
    }

    pub fn or(left: LockComposition, right: LockComposition) -> Self {
        LockComposition::Or(Box::new(left), Box::new(right))
    }

    pub fn not(inner: LockComposition) -> Self {
        LockComposition::Not(Box::new(inner))
    }

    /// Evaluate this composition against a set of satisfied lock IDs.
    pub fn evaluate(&self, satisfied: &HashSet<&str>) -> bool {
        match self {
            LockComposition::Single(id) => satisfied.contains(id.as_str()),
            LockComposition::And(l, r) => l.evaluate(satisfied) && r.evaluate(satisfied),
            LockComposition::Or(l, r) => l.evaluate(satisfied) || r.evaluate(satisfied),
            LockComposition::Not(inner) => !inner.evaluate(satisfied),
        }
    }

    /// Collect all referenced lock IDs.
    pub fn referenced_ids(&self) -> HashSet<String> {
        match self {
            LockComposition::Single(id) => {
                let mut set = HashSet::new();
                set.insert(id.clone());
                set
            }
            LockComposition::And(l, r) | LockComposition::Or(l, r) => {
                let mut set = l.referenced_ids();
                set.extend(r.referenced_ids());
                set
            }
            LockComposition::Not(inner) => inner.referenced_ids(),
        }
    }

    /// Depth of the composition tree.
    pub fn depth(&self) -> usize {
        match self {
            LockComposition::Single(_) => 1,
            LockComposition::And(l, r) | LockComposition::Or(l, r) => {
                1 + l.depth().max(r.depth())
            }
            LockComposition::Not(inner) => 1 + inner.depth(),
        }
    }
}

/// Compress a set of locks by merging overlapping patterns.
pub fn compress_locks(locks: &[Lock]) -> Vec<Lock> {
    if locks.is_empty() { return vec![]; }

    let mut compressed: Vec<Lock> = Vec::new();
    let mut used: HashSet<usize> = HashSet::new();

    for i in 0..locks.len() {
        if used.contains(&i) { continue; }
        let mut merged = locks[i].clone();
        for j in (i + 1)..locks.len() {
            if used.contains(&j) { continue; }
            if can_merge(&merged, &locks[j]) {
                merged = merge_patterns(&merged, &locks[j]);
                used.insert(j);
            }
        }
        compressed.push(merged);
    }
    compressed
}

fn can_merge(a: &Lock, b: &Lock) -> bool {
    if a.pattern.len() != b.pattern.len() { return false; }
    // Can merge if patterns agree where both are non-zero
    a.pattern.iter().zip(b.pattern.iter()).all(|(x, y)| {
        *x == 0 || *y == 0 || *x == *y
    })
}

fn merge_patterns(a: &Lock, b: &Lock) -> Lock {
    let pattern: Pattern = a.pattern.iter().zip(b.pattern.iter()).map(|(x, y)| {
        if *x != 0 { *x } else { *y }
    }).collect();
    Lock {
        id: format!("{}|{}", a.id, b.id),
        pattern,
        strength: a.strength + b.strength,
        active: a.active && b.active,
    }
}

/// Score cross-model transfer between two lock sets.
/// Higher score means more transferable patterns.
pub fn transfer_score(source: &[Lock], target: &[Lock]) -> f64 {
    if source.is_empty() || target.is_empty() { return 0.0; }

    let mut matches = 0;
    let mut total_comparisons = 0;

    for s in source {
        for t in target {
            total_comparisons += 1;
            if s.pattern.len() == t.pattern.len() {
                let agreements = s.pattern.iter().zip(t.pattern.iter())
                    .filter(|(a, b)| **a == **b && **a != 0)
                    .count();
                let constraints = s.pattern.iter().chain(t.pattern.iter())
                    .filter(|&&x| x != 0)
                    .count();
                if constraints > 0 && agreements > 0 {
                    matches += 1;
                }
            }
        }
    }

    if total_comparisons == 0 { 0.0 } else { matches as f64 / total_comparisons as f64 }
}

/// Detect critical mass: when enough locks cover a pattern space sufficiently.
pub fn detect_critical_mass(locks: &[Lock], pattern_length: usize, threshold: f64) -> bool {
    if locks.is_empty() || pattern_length == 0 { return false; }

    // Total possible patterns: 3^pattern_length
    // Each lock covers patterns it satisfies
    let active_locks: Vec<&Lock> = locks.iter().filter(|l| l.active).collect();
    if active_locks.is_empty() { return false; }

    let total_patterns = 3usize.pow(pattern_length as u32);
    if total_patterns > 10000 {
        // For large pattern spaces, use coverage ratio heuristic
        let coverage = active_locks.len() as f64 * 3.0 / pattern_length as f64;
        return coverage >= threshold;
    }

    // Enumerate all patterns and check coverage
    let mut covered = 0;
    for i in 0..total_patterns {
        let mut pattern = Pattern::with_capacity(pattern_length);
        let mut val = i;
        for _ in 0..pattern_length {
            pattern.push(match val % 3 {
                0 => 0,
                1 => 1,
                2 => -1,
                _ => unreachable!(),
            });
            val /= 3;
        }
        if active_locks.iter().any(|l| l.satisfies(&pattern)) {
            covered += 1;
        }
    }

    let ratio = covered as f64 / total_patterns as f64;
    ratio >= threshold
}

/// A dependency graph of locks.
#[derive(Clone, Debug)]
pub struct LockGraph {
    /// Adjacency: lock_id -> set of lock_ids it depends on
    deps: HashMap<String, HashSet<String>>,
    locks: HashMap<String, Lock>,
}

impl LockGraph {
    pub fn new() -> Self {
        LockGraph {
            deps: HashMap::new(),
            locks: HashMap::new(),
        }
    }

    pub fn add_lock(&mut self, lock: Lock) {
        self.deps.entry(lock.id.clone()).or_default();
        self.locks.insert(lock.id.clone(), lock);
    }

    pub fn add_dependency(&mut self, from: impl Into<String>, to: impl Into<String>) {
        let from = from.into();
        let to = to.into();
        self.deps.entry(from.clone()).or_default().insert(to.clone());
        self.deps.entry(to).or_default();
    }

    /// Topological sort of locks respecting dependencies.
    pub fn topological_order(&self) -> Vec<String> {
        let mut in_degree: HashMap<&str, usize> = HashMap::new();
        for id in self.deps.keys() {
            in_degree.entry(id.as_str()).or_insert(0);
        }
        for deps in self.deps.values() {
            for dep in deps {
                *in_degree.entry(dep.as_str()).or_insert(0) += 1;
            }
        }

        let mut queue: VecDeque<&str> = in_degree.iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&id, _)| id)
            .collect();

        let mut result = Vec::new();
        while let Some(id) = queue.pop_front() {
            result.push(id.to_string());
            if let Some(deps) = self.deps.get(id) {
                for dep in deps {
                    // Reverse: actually deps means "from depends on to"
                    // So we need reverse edges for topo sort
                }
            }
        }

        // Simpler approach: Kahn's on reverse graph
        let mut reverse: HashMap<&str, HashSet<&str>> = HashMap::new();
        let mut in_deg: HashMap<&str, usize> = HashMap::new();
        for id in self.deps.keys() {
            reverse.entry(id.as_str()).or_default();
            in_deg.entry(id.as_str()).or_insert(0);
        }
        for (id, deps) in &self.deps {
            for dep in deps {
                reverse.entry(dep.as_str()).or_default().insert(id.as_str());
                *in_deg.entry(id.as_str()).or_insert(0) += 1;
            }
        }

        let mut queue: VecDeque<&str> = in_deg.iter()
            .filter(|(_, &d)| d == 0)
            .map(|(&id, _)| id)
            .collect();

        result.clear();
        while let Some(id) = queue.pop_front() {
            result.push(id.to_string());
            if let Some(successors) = reverse.get(id) {
                for &succ in successors {
                    let deg = in_deg.get_mut(succ).unwrap();
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(succ);
                    }
                }
            }
        }
        result
    }

    /// Find all locks that depend on a given lock (transitive).
    pub fn dependents(&self, id: &str) -> HashSet<String> {
        let mut result = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(id.to_string());

        while let Some(current) = queue.pop_front() {
            for (lock_id, deps) in &self.deps {
                if deps.contains(&current) && !result.contains(lock_id) {
                    result.insert(lock_id.clone());
                    queue.push_back(lock_id.clone());
                }
            }
        }
        result
    }

    /// Detect cycles in the dependency graph.
    pub fn has_cycles(&self) -> bool {
        let order = self.topological_order();
        order.len() < self.deps.len()
    }

    pub fn lock_count(&self) -> usize {
        self.locks.len()
    }
}

/// Grave-yard: analysis of expired (inactive) locks.
#[derive(Clone, Debug)]
pub struct Graveyard {
    expired: Vec<Lock>,
    pub expired_at_step: HashMap<String, usize>,
}

impl Graveyard {
    pub fn new() -> Self {
        Graveyard {
            expired: Vec::new(),
            expired_at_step: HashMap::new(),
        }
    }

    /// Bury a lock (move to graveyard).
    pub fn bury(&mut self, mut lock: Lock, step: usize) {
        lock.expire();
        self.expired_at_step.insert(lock.id.clone(), step);
        self.expired.push(lock);
    }

    pub fn count(&self) -> usize {
        self.expired.len()
    }

    /// Average strength of expired locks.
    pub fn avg_strength(&self) -> f64 {
        if self.expired.is_empty() { return 0.0; }
        self.expired.iter().map(|l| l.strength as f64).sum::<f64>() / self.expired.len() as f64
    }

    /// Find locks that were strongest when expired.
    pub fn strongest(&self, n: usize) -> Vec<&Lock> {
        let mut sorted: Vec<&Lock> = self.expired.iter().collect();
        sorted.sort_by(|a, b| b.strength.cmp(&a.strength));
        sorted.into_iter().take(n).collect()
    }

    /// Pattern frequency analysis: which pattern elements were most common in expired locks.
    pub fn pattern_frequency(&self) -> HashMap<i8, usize> {
        let mut freq: HashMap<i8, usize> = HashMap::new();
        for lock in &self.expired {
            for &val in &lock.pattern {
                *freq.entry(val).or_insert(0) += 1;
            }
        }
        freq
    }

    /// Archaeology: find locks that could have been revived (patterns overlapping with active locks).
    pub fn revivable(&self, active: &[Lock]) -> Vec<&Lock> {
        self.expired.iter().filter(|expired| {
            active.iter().any(|a| can_merge(a, expired))
        }).collect()
    }

    /// Half-life: step at which half of all buried locks had been buried.
    pub fn half_life(&self) -> Option<usize> {
        if self.expired_at_step.is_empty() { return None; }
        let mut steps: Vec<usize> = self.expired_at_step.values().copied().collect();
        steps.sort();
        let mid = steps.len() / 2;
        Some(steps[mid])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lock_creation() {
        let lock = Lock::new("test", vec![1, 0, -1]);
        assert_eq!(lock.id, "test");
        assert_eq!(lock.pattern, vec![1, 0, -1]);
        assert_eq!(lock.strength, 1);
        assert!(lock.active);
    }

    #[test]
    fn test_lock_satisfies_exact() {
        let lock = Lock::new("a", vec![1, -1, 0]);
        assert!(lock.satisfies(&vec![1, -1, 0]));
    }

    #[test]
    fn test_lock_satisfies_wildcard() {
        let lock = Lock::new("a", vec![1, 0, -1]);
        assert!(lock.satisfies(&vec![1, 1, -1])); // position 1 is wildcard
        assert!(lock.satisfies(&vec![1, -1, -1]));
    }

    #[test]
    fn test_lock_not_satisfies() {
        let lock = Lock::new("a", vec![1, 0, -1]);
        assert!(!lock.satisfies(&vec![-1, 0, -1])); // position 0 doesn't match
    }

    #[test]
    fn test_lock_wrong_length() {
        let lock = Lock::new("a", vec![1, 0]);
        assert!(!lock.satisfies(&vec![1, 0, 0]));
    }

    #[test]
    fn test_lock_specificity() {
        let specific = Lock::new("s", vec![1, -1, 0]);
        let general = Lock::new("g", vec![1, 0, 0]);
        assert!(specific.is_more_specific_than(&general));
    }

    #[test]
    fn test_lock_expire() {
        let mut lock = Lock::new("a", vec![1]);
        lock.expire();
        assert!(!lock.active);
    }

    #[test]
    fn test_composition_single() {
        let comp = LockComposition::single("a");
        let mut satisfied = HashSet::new();
        assert!(!comp.evaluate(&satisfied));
        satisfied.insert("a");
        assert!(comp.evaluate(&satisfied));
    }

    #[test]
    fn test_composition_and() {
        let comp = LockComposition::and(
            LockComposition::single("a"),
            LockComposition::single("b"),
        );
        let mut s = HashSet::new();
        assert!(!comp.evaluate(&s));
        s.insert("a");
        assert!(!comp.evaluate(&s));
        s.insert("b");
        assert!(comp.evaluate(&s));
    }

    #[test]
    fn test_composition_or() {
        let comp = LockComposition::or(
            LockComposition::single("a"),
            LockComposition::single("b"),
        );
        let mut s = HashSet::new();
        assert!(!comp.evaluate(&s));
        s.insert("a");
        assert!(comp.evaluate(&s));
    }

    #[test]
    fn test_composition_not() {
        let comp = LockComposition::not(LockComposition::single("a"));
        let mut s = HashSet::new();
        assert!(comp.evaluate(&s));
        s.insert("a");
        assert!(!comp.evaluate(&s));
    }

    #[test]
    fn test_composition_referenced_ids() {
        let comp = LockComposition::and(
            LockComposition::single("a"),
            LockComposition::or(
                LockComposition::single("b"),
                LockComposition::single("c"),
            ),
        );
        let ids = comp.referenced_ids();
        assert_eq!(ids.len(), 3);
        assert!(ids.contains("a"));
        assert!(ids.contains("b"));
        assert!(ids.contains("c"));
    }

    #[test]
    fn test_composition_depth() {
        let comp = LockComposition::and(
            LockComposition::single("a"),
            LockComposition::single("b"),
        );
        assert_eq!(comp.depth(), 2);
    }

    #[test]
    fn test_compress_locks() {
        let locks = vec![
            Lock::new("a", vec![1, 0, 0]),
            Lock::new("b", vec![0, -1, 0]),
        ];
        let compressed = compress_locks(&locks);
        // Can merge (no conflicts)
        assert!(compressed.len() <= 2);
    }

    #[test]
    fn test_compress_no_merge_conflict() {
        let locks = vec![
            Lock::new("a", vec![1, 0]),
            Lock::new("b", vec![-1, 0]),
        ];
        let compressed = compress_locks(&locks);
        assert_eq!(compressed.len(), 2); // Can't merge: position 0 conflicts
    }

    #[test]
    fn test_transfer_score_identical() {
        let source = vec![Lock::new("a", vec![1, -1])];
        let target = vec![Lock::new("b", vec![1, -1])];
        let score = transfer_score(&source, &target);
        assert!(score > 0.0);
    }

    #[test]
    fn test_transfer_score_different() {
        let source = vec![Lock::new("a", vec![1, 1])];
        let target = vec![Lock::new("b", vec![-1, -1])];
        let score = transfer_score(&source, &target);
        assert_eq!(score, 0.0); // No agreements on non-zero positions
    }

    #[test]
    fn test_critical_mass_small_patterns() {
        let locks = vec![
            Lock::new("a", vec![1, 0]),
            Lock::new("b", vec![0, 1]),
            Lock::new("c", vec![-1, 0]),
            Lock::new("d", vec![0, -1]),
        ];
        // Should detect critical mass for small pattern space
        let result = detect_critical_mass(&locks, 2, 0.5);
        assert!(result);
    }

    #[test]
    fn test_lock_graph_basic() {
        let mut graph = LockGraph::new();
        graph.add_lock(Lock::new("a", vec![1]));
        graph.add_lock(Lock::new("b", vec![-1]));
        graph.add_dependency("b", "a");
        assert_eq!(graph.lock_count(), 2);
    }

    #[test]
    fn test_lock_graph_topo_sort() {
        let mut graph = LockGraph::new();
        graph.add_lock(Lock::new("a", vec![1]));
        graph.add_lock(Lock::new("b", vec![-1]));
        graph.add_lock(Lock::new("c", vec![0]));
        graph.add_dependency("b", "a");
        graph.add_dependency("c", "b");
        let order = graph.topological_order();
        assert_eq!(order.len(), 3);
        // a should come before b, b before c
        let pos_a = order.iter().position(|x| x == "a").unwrap();
        let pos_b = order.iter().position(|x| x == "b").unwrap();
        let pos_c = order.iter().position(|x| x == "c").unwrap();
        assert!(pos_a < pos_b);
        assert!(pos_b < pos_c);
    }

    #[test]
    fn test_lock_graph_dependents() {
        let mut graph = LockGraph::new();
        graph.add_lock(Lock::new("a", vec![1]));
        graph.add_lock(Lock::new("b", vec![-1]));
        graph.add_lock(Lock::new("c", vec![0]));
        graph.add_dependency("b", "a");
        graph.add_dependency("c", "a");
        let deps = graph.dependents("a");
        assert!(deps.contains("b"));
        assert!(deps.contains("c"));
    }

    #[test]
    fn test_lock_graph_no_cycle() {
        let mut graph = LockGraph::new();
        graph.add_lock(Lock::new("a", vec![1]));
        graph.add_lock(Lock::new("b", vec![-1]));
        graph.add_dependency("b", "a");
        assert!(!graph.has_cycles());
    }

    #[test]
    fn test_lock_graph_cycle() {
        let mut graph = LockGraph::new();
        graph.add_lock(Lock::new("a", vec![1]));
        graph.add_lock(Lock::new("b", vec![-1]));
        graph.add_dependency("a", "b");
        graph.add_dependency("b", "a");
        assert!(graph.has_cycles());
    }

    #[test]
    fn test_graveyard_bury() {
        let mut gy = Graveyard::new();
        gy.bury(Lock::new("a", vec![1, -1]).with_strength(5), 10);
        assert_eq!(gy.count(), 1);
        assert_eq!(gy.expired_at_step.get("a"), Some(&10));
    }

    #[test]
    fn test_graveyard_avg_strength() {
        let mut gy = Graveyard::new();
        gy.bury(Lock::new("a", vec![1]).with_strength(4), 1);
        gy.bury(Lock::new("b", vec![-1]).with_strength(6), 2);
        assert!((gy.avg_strength() - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_graveyard_strongest() {
        let mut gy = Graveyard::new();
        gy.bury(Lock::new("a", vec![1]).with_strength(3), 1);
        gy.bury(Lock::new("b", vec![-1]).with_strength(7), 2);
        gy.bury(Lock::new("c", vec![0]).with_strength(5), 3);
        let top = gy.strongest(2);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].strength, 7);
    }

    #[test]
    fn test_graveyard_pattern_frequency() {
        let mut gy = Graveyard::new();
        gy.bury(Lock::new("a", vec![1, 1, -1]), 1);
        gy.bury(Lock::new("b", vec![1, 0, 0]), 2);
        let freq = gy.pattern_frequency();
        assert_eq!(*freq.get(&1).unwrap(), 3);
    }

    #[test]
    fn test_graveyard_revivable() {
        let mut gy = Graveyard::new();
        gy.bury(Lock::new("a", vec![1, 0, -1]).with_strength(5), 1);
        let active = vec![Lock::new("b", vec![0, -1, -1])];
        let revivable = gy.revivable(&active);
        assert_eq!(revivable.len(), 1);
    }
}
