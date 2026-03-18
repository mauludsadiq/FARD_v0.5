//! Inherit-Cert CRDT
//!
//! A Min-Register Map CRDT for convergent receipt canonicalization.
//!
//! ## Formal specification
//!
//! State: `Map<EffectKey, MinRegister<RunID>>`
//!
//! - `EffectKey` = `phi(e)` = SHA-256 of canonical effect encoding
//! - `RunID` = SHA-256 content identifier of a FARD run ("sha256:...")
//! - `MinRegister<T>` = a register whose merge is pointwise minimum
//!   (totally ordered by lexicographic byte order of the hex digest)
//!
//! ## Semilattice laws
//!
//! Let S = InheritCertState. For all a, b, c: S:
//!
//! 1. Idempotent:   merge(a, a) = a
//! 2. Commutative:  merge(a, b) = merge(b, a)
//! 3. Associative:  merge(merge(a,b), c) = merge(a, merge(b,c))
//! 4. Monotone:     a <= merge(a, b)  (where <= is the natural partial order)
//!
//! All four laws are verified by property tests in this module.
//!
//! ## Convergence guarantee
//!
//! Given any two replicas R1, R2 that have seen the same set of effects
//! and proposed RunIDs, after one round of merge they hold identical state.
//! The canonical RunID for each effect is the lexicographic minimum over all
//! proposals — a deterministic, replica-independent choice.

use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};

// ── Effect key ────────────────────────────────────────────────────────────────

/// The canonical key for an effect: SHA-256 of the effect's canonical encoding.
/// Corresponds to phi(e) in the formal spec.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EffectKey(pub String); // "sha256:<hex>"

impl EffectKey {
    /// Compute effect key from raw bytes of canonical effect encoding.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        use sha2::Digest;
        let mut h = sha2::Sha256::new();
        h.update(bytes);
        let result = h.finalize();
        let hex: String = result.iter().map(|b| format!("{:02x}", b)).collect();
        EffectKey(format!("sha256:{}", hex))
    }

    /// Compute effect key from a kind string and canonical request bytes.
    /// Layout: UTF8(kind) || 0x00 || canonical_req_bytes
    pub fn from_kind_req(kind: &str, req_bytes: &[u8]) -> Self {
        let mut buf = Vec::with_capacity(kind.len() + 1 + req_bytes.len());
        buf.extend_from_slice(kind.as_bytes());
        buf.push(0u8);
        buf.extend_from_slice(req_bytes);
        Self::from_bytes(&buf)
    }

    pub fn as_str(&self) -> &str { &self.0 }
}

impl std::fmt::Display for EffectKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ── RunID ─────────────────────────────────────────────────────────────────────

/// A FARD run identifier: "sha256:<64 hex chars>"
/// Totally ordered by lexicographic byte order of the hex digest.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RunID(pub String);

impl RunID {
    pub fn new(s: impl Into<String>) -> Self { RunID(s.into()) }
    pub fn as_str(&self) -> &str { &self.0 }

    /// Validate that this is a well-formed RunID.
    pub fn is_valid(&self) -> bool {
        self.0.starts_with("sha256:")
            && self.0.len() == 71
            && self.0[7..].chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
    }
}

impl std::fmt::Display for RunID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ── MinRegister ───────────────────────────────────────────────────────────────

/// A single-value register whose merge operation is min.
/// The canonical value is the lexicographic minimum over all proposals.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MinRegister<T: Ord + Clone> {
    pub value: T,
}

impl<T: Ord + Clone> MinRegister<T> {
    pub fn new(value: T) -> Self { MinRegister { value } }

    /// Merge: keep the minimum value.
    pub fn merge(&self, other: &Self) -> Self {
        MinRegister {
            value: self.value.clone().min(other.value.clone()),
        }
    }

    /// Semilattice partial order: a <= b iff merge(a,b) = b
    /// For MinRegister, merge = min, so merge(a,b)=b means a.value >= b.value
    /// (smaller value = more information = higher in lattice)
    pub fn leq(&self, other: &Self) -> bool {
        self.value >= other.value
    }
}

// ── InheritCertState ──────────────────────────────────────────────────────────

/// The full CRDT state: a map from effect keys to their canonical RunID.
/// Merge is pointwise minimum over all registers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct InheritCertState {
    /// Map from effect key to the minimum RunID seen for that effect.
    pub certs: BTreeMap<EffectKey, MinRegister<RunID>>,
}

impl InheritCertState {
    pub fn new() -> Self { Self::default() }

    /// Propose a RunID for an effect key.
    /// If the effect is new, records the proposal.
    /// If the effect exists, keeps the minimum.
    pub fn propose(&mut self, key: EffectKey, run_id: RunID) {
        let entry = self.certs.entry(key).or_insert_with(|| MinRegister::new(run_id.clone()));
        if run_id < entry.value {
            entry.value = run_id;
        }
    }

    /// Merge another state into this one (pointwise minimum).
    /// This is the CRDT join operation.
    pub fn merge(&self, other: &Self) -> Self {
        let mut result = self.clone();
        for (key, reg) in &other.certs {
            match result.certs.get_mut(key) {
                Some(existing) => {
                    *existing = existing.merge(reg);
                }
                None => {
                    result.certs.insert(key.clone(), reg.clone());
                }
            }
        }
        result
    }

    /// Merge in place.
    pub fn merge_into(&mut self, other: &Self) {
        for (key, reg) in &other.certs {
            match self.certs.get_mut(key) {
                Some(existing) => {
                    *existing = existing.merge(reg);
                }
                None => {
                    self.certs.insert(key.clone(), reg.clone());
                }
            }
        }
    }

    /// Semilattice partial order: self <= other iff merge(self, other) = other
    /// For each key in self, other must have an equal or smaller (more canonical) RunID.
    /// Keys in other but not in self are fine (other has more information).
    pub fn leq(&self, other: &Self) -> bool {
        for (key, reg) in &self.certs {
            match other.certs.get(key) {
                Some(other_reg) => {
                    // reg.leq(other_reg) means reg.value >= other_reg.value
                    if !reg.leq(other_reg) { return false; }
                }
                // Key in self but not in other — self has info other doesn't
                // merge(self,other) would add this key, != other, so self not <= other
                None => return false,
            }
        }
        true
    }

    /// Look up the canonical RunID for an effect.
    pub fn get(&self, key: &EffectKey) -> Option<&RunID> {
        self.certs.get(key).map(|r| &r.value)
    }

    /// Number of effects tracked.
    pub fn len(&self) -> usize { self.certs.len() }
    pub fn is_empty(&self) -> bool { self.certs.is_empty() }

    /// All (effect_key, canonical_run_id) pairs, sorted by effect key.
    pub fn entries(&self) -> Vec<(&EffectKey, &RunID)> {
        self.certs.iter().map(|(k, r)| (k, &r.value)).collect()
    }

    /// Serialize to JSON wire format.
    pub fn to_json(&self) -> serde_json::Value {
        let entries: serde_json::Map<String, serde_json::Value> = self.certs.iter()
            .map(|(k, r)| (k.0.clone(), serde_json::Value::String(r.value.0.clone())))
            .collect();
        serde_json::json!({
            "kind": "fard/inherit_cert/v0.1",
            "certs": entries,
        })
    }

    /// Deserialize from JSON wire format.
    pub fn from_json(v: &serde_json::Value) -> Result<Self, String> {
        let certs_obj = v.get("certs")
            .and_then(|v| v.as_object())
            .ok_or("missing certs object")?;
        let mut state = Self::new();
        for (k, v) in certs_obj {
            let run_id_str = v.as_str().ok_or("cert value must be string")?;
            let key = EffectKey(k.clone());
            let run_id = RunID(run_id_str.to_string());
            if !run_id.is_valid() {
                return Err(format!("invalid RunID: {}", run_id_str));
            }
            state.certs.insert(key, MinRegister::new(run_id));
        }
        Ok(state)
    }
}

// ── Delta operations ──────────────────────────────────────────────────────────

/// A delta: a minimal state update that can be sent between replicas.
/// Contains only the keys that changed relative to a known state.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InheritCertDelta {
    pub updates: BTreeMap<EffectKey, RunID>,
}

impl InheritCertDelta {
    pub fn new() -> Self { InheritCertDelta { updates: BTreeMap::new() } }

    /// Compute the delta needed to bring `other` up to `self`.
    /// Returns updates that are strictly smaller in self than in other.
    pub fn compute(from: &InheritCertState, to: &InheritCertState) -> Self {
        let mut delta = Self::new();
        for (key, reg) in &to.certs {
            match from.certs.get(key) {
                Some(existing) if existing.value <= reg.value => {
                    // from already has a smaller or equal value — no update needed
                }
                _ => {
                    delta.updates.insert(key.clone(), reg.value.clone());
                }
            }
        }
        delta
    }

    /// Apply a delta to a state.
    pub fn apply_to(&self, state: &mut InheritCertState) {
        for (key, run_id) in &self.updates {
            state.propose(key.clone(), run_id.clone());
        }
    }

    pub fn is_empty(&self) -> bool { self.updates.is_empty() }
    pub fn len(&self) -> usize { self.updates.len() }
}

impl Default for InheritCertDelta {
    fn default() -> Self { Self::new() }
}

// ── Semilattice law verification ──────────────────────────────────────────────

/// Check all four semilattice laws for a given set of states.
/// Returns Ok(()) if all laws hold, Err(description) if any fail.
pub fn verify_semilattice_laws(
    a: &InheritCertState,
    b: &InheritCertState,
    c: &InheritCertState,
) -> Result<(), String> {
    // Law 1: Idempotent — merge(a, a) = a
    let aa = a.merge(a);
    if aa != *a {
        return Err(format!("IDEMPOTENT_FAIL: merge(a,a) != a"));
    }

    // Law 2: Commutative — merge(a, b) = merge(b, a)
    let ab = a.merge(b);
    let ba = b.merge(a);
    if ab != ba {
        return Err(format!("COMMUTATIVE_FAIL: merge(a,b) != merge(b,a)"));
    }

    // Law 3: Associative — merge(merge(a,b), c) = merge(a, merge(b,c))
    let ab_c = a.merge(b).merge(c);
    let a_bc = a.merge(&b.merge(c));
    if ab_c != a_bc {
        return Err(format!("ASSOCIATIVE_FAIL: merge(merge(a,b),c) != merge(a,merge(b,c))"));
    }

    // Law 4: Monotone — a <= merge(a, b)
    if !a.leq(&a.merge(b)) {
        return Err(format!("MONOTONE_FAIL: a not <= merge(a,b)"));
    }
    if !b.leq(&a.merge(b)) {
        return Err(format!("MONOTONE_FAIL: b not <= merge(a,b)"));
    }

    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_run(n: u8) -> RunID {
        RunID(format!("sha256:{:064x}", n))
    }

    fn make_key(s: &str) -> EffectKey {
        EffectKey::from_bytes(s.as_bytes())
    }

    #[test]
    fn test_min_register_merge() {
        let r1 = MinRegister::new(make_run(5));
        let r2 = MinRegister::new(make_run(3));
        let merged = r1.merge(&r2);
        assert_eq!(merged.value, make_run(3)); // min wins
    }

    #[test]
    fn test_propose_keeps_min() {
        let mut state = InheritCertState::new();
        let key = make_key("effect_a");
        state.propose(key.clone(), make_run(10));
        state.propose(key.clone(), make_run(5));
        state.propose(key.clone(), make_run(8));
        assert_eq!(state.get(&key), Some(&make_run(5)));
    }

    #[test]
    fn test_semilattice_laws_basic() {
        let mut a = InheritCertState::new();
        let mut b = InheritCertState::new();
        let mut c = InheritCertState::new();

        a.propose(make_key("e1"), make_run(3));
        a.propose(make_key("e2"), make_run(7));
        b.propose(make_key("e1"), make_run(1));
        b.propose(make_key("e3"), make_run(9));
        c.propose(make_key("e2"), make_run(4));
        c.propose(make_key("e3"), make_run(2));

        verify_semilattice_laws(&a, &b, &c).expect("semilattice laws must hold");
    }

    #[test]
    fn test_convergence() {
        // Two replicas see different proposals — after merge they agree
        let mut r1 = InheritCertState::new();
        let mut r2 = InheritCertState::new();

        let key = make_key("http_get_example_com");
        r1.propose(key.clone(), make_run(0xAB));
        r2.propose(key.clone(), make_run(0x07));

        // Neither replica alone has the canonical answer
        assert_ne!(r1.get(&key), r2.get(&key));

        // After one round of merge, both agree
        let merged_1 = r1.merge(&r2);
        let merged_2 = r2.merge(&r1);
        assert_eq!(merged_1, merged_2);
        assert_eq!(merged_1.get(&key), Some(&make_run(0x07))); // min wins
    }

    #[test]
    fn test_delta_sync() {
        let mut r1 = InheritCertState::new();
        let mut r2 = InheritCertState::new();

        r1.propose(make_key("e1"), make_run(5));
        r1.propose(make_key("e2"), make_run(3));
        r2.propose(make_key("e2"), make_run(1));
        r2.propose(make_key("e3"), make_run(7));

        // Compute delta from r1's perspective (what r2 needs)
        let delta = InheritCertDelta::compute(&r1, &r2);
        delta.apply_to(&mut r1);

        // Now r1 should equal merge(r1_orig, r2)
        let mut r1_orig = InheritCertState::new();
        r1_orig.propose(make_key("e1"), make_run(5));
        r1_orig.propose(make_key("e2"), make_run(3));
        let expected = r1_orig.merge(&r2);
        assert_eq!(r1, expected);
    }

    #[test]
    fn test_json_roundtrip() {
        let mut state = InheritCertState::new();
        state.propose(make_key("e1"), make_run(1));
        state.propose(make_key("e2"), make_run(2));

        let json = state.to_json();
        let restored = InheritCertState::from_json(&json).expect("roundtrip");
        assert_eq!(state, restored);
    }

    #[test]
    fn test_effect_key_from_kind_req() {
        let k1 = EffectKey::from_kind_req("http_get", b"https://example.com");
        let k2 = EffectKey::from_kind_req("http_get", b"https://example.com");
        let k3 = EffectKey::from_kind_req("http_get", b"https://other.com");
        assert_eq!(k1, k2);
        assert_ne!(k1, k3);
    }

    #[test]
    fn test_empty_state_laws() {
        let a = InheritCertState::new();
        let b = InheritCertState::new();
        let c = InheritCertState::new();
        verify_semilattice_laws(&a, &b, &c).expect("empty state laws");
    }

    #[test]
    fn test_run_id_ordering() {
        // RunIDs are ordered lexicographically by their hex digest
        // "sha256:000..." < "sha256:aaa..." < "sha256:fff..."
        let r_low  = make_run(0x00);
        let r_mid  = make_run(0xAA);
        let r_high = make_run(0xFF);
        assert!(r_low < r_mid);
        assert!(r_mid < r_high);
    }
}
