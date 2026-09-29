//! The action library (ARCHITECTURE §6.2 step 5, §6.4; CONVENTIONS §7): action expansion by
//! setups, and `library.json` in canonical form with its sha256.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use nx_sim::OrbitKind;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::MACRO_KINDS;
use crate::discover::{Found, Probe, SearchConfig, SearchStats, discover, sequences};
use crate::effect::{Effect, MacroKind, orientation_mod};
use crate::sym::{self, LayerRef, SymMove};

/// Bump on any change to slot conventions, action ordering or JSON semantics (CONVENTIONS §7).
pub const LIBRARY_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Library {
    pub library_version: u32,
    pub sha256: String,
    pub generator: Generator,
    pub types: BTreeMap<String, TypeLibrary>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Generator {
    /// Commit the library was generated at. Informational: excluded from `sha256`.
    pub git_commit: String,
    /// Search limits per type.
    pub search: BTreeMap<String, SearchConfig>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeLibrary {
    pub type_id: u8,
    /// `"piece"` (content = piece·orientation_mod + orientation) or `"color"`.
    pub content: String,
    pub orientation_mod: u8,
    pub slots: usize,
    pub layer_refs: Vec<String>,
    pub macros: Vec<MacroEntry>,
    pub actions: Vec<ActionEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacroEntry {
    pub id: u32,
    pub moves: Vec<String>,
    /// `three_cycle`, `twist_pair` or `flip_pair`.
    pub kind: String,
    /// The slots it touches: `[a, b, c]` for a 3-cycle (piece at a goes to b, b to c, c to a),
    /// or the two slots of an orientation pair.
    pub cycle: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionEntry {
    pub id: u32,
    pub setup: Vec<String>,
    #[serde(rename = "macro")]
    pub macro_id: u32,
    pub cost: u32,
    pub perm: Vec<u8>,
    pub ori_delta: Vec<u8>,
}

impl TypeLibrary {
    pub fn kind(&self, name: &str) -> Option<OrbitKind> {
        OrbitKind::from_name(name).filter(|k| k.type_id() == Some(self.type_id))
    }

    /// Primitive symbolic moves of an action: `S · M · S⁻¹`, cancelled.
    pub fn action_moves(&self, a: &ActionEntry) -> Result<Vec<SymMove>, String> {
        let setup = sym::parse_seq(&a.setup).map_err(|e| e.to_string())?;
        let m = self
            .macros
            .get(a.macro_id as usize)
            .ok_or_else(|| format!("action {} names missing macro {}", a.id, a.macro_id))?;
        let body = sym::parse_seq(&m.moves).map_err(|e| e.to_string())?;
        Ok(sym::conjugate(&setup, &body))
    }

    pub fn effect(a: &ActionEntry) -> Effect {
        Effect {
            perm: a.perm.clone(),
            ori: a.ori_delta.clone(),
        }
    }
}

/// Per-type report of a build.
#[derive(Clone, Debug)]
pub struct BuildReport {
    pub kind: OrbitKind,
    pub stats: SearchStats,
    pub macros_found: usize,
    pub macros_used: usize,
    pub actions: usize,
    pub by_kind: BTreeMap<String, usize>,
    pub mean_cost: f64,
    pub max_cost: u32,
}

/// Discover and expand every type. Deterministic for fixed configs.
pub fn build(git_commit: &str) -> (Library, Vec<BuildReport>) {
    let mut types = BTreeMap::new();
    let mut search = BTreeMap::new();
    let mut reports = Vec::new();
    for kind in MACRO_KINDS {
        let cfg = SearchConfig::default_for(kind);
        let (found, stats) = discover(kind, &cfg);
        let (lib, report) = expand(kind, &cfg, &found, stats);
        types.insert(kind.name().to_string(), lib);
        search.insert(kind.name().to_string(), cfg);
        reports.push(report);
    }
    let mut lib = Library {
        library_version: LIBRARY_VERSION,
        sha256: String::new(),
        generator: Generator {
            git_commit: git_commit.to_string(),
            search,
        },
        types,
    };
    lib.sha256 = lib.content_hash();
    (lib, reports)
}

/// Expand macros into actions `S·M·S⁻¹` over all setups of length ≤ `cfg.setup_len`; keep
/// the cheapest per distinct slot effect.
pub fn expand(
    kind: OrbitKind,
    cfg: &SearchConfig,
    found: &[Found],
    stats: SearchStats,
) -> (TypeLibrary, BuildReport) {
    let probe = Probe::new(kind, cfg);
    let m = orientation_mod(kind);
    let setups: Vec<Vec<SymMove>> = (0..=cfg.setup_len)
        .flat_map(|l| sequences(&probe.gens, l))
        .map(|s| s.iter().map(|&i| probe.gens[usize::from(i)]).collect())
        .collect();
    let setup_effects: Vec<(Effect, Effect)> = setups
        .iter()
        .map(|s| {
            let e = probe.effect_of_seq(s);
            let inv = e.inverse(m);
            (e, inv)
        })
        .collect();
    // Key: (cost, macro index, setup index), smallest wins.
    type Best = HashMap<Effect, (u32, usize, usize)>;
    let merge = |mut a: Best, b: Best| {
        for (e, k) in b {
            a.entry(e)
                .and_modify(|old| *old = (*old).min(k))
                .or_insert(k);
        }
        a
    };
    let best: Best = found
        .par_iter()
        .enumerate()
        .fold(Best::new, |mut acc, (mi, f)| {
            for (si, (s, (se, si_inv))) in setups.iter().zip(&setup_effects).enumerate() {
                let effect = se.then(&f.effect, m).then(si_inv, m);
                let cost = sym::conjugate(s, &f.moves).len() as u32;
                let key = (cost, mi, si);
                acc.entry(effect)
                    .and_modify(|old| *old = (*old).min(key))
                    .or_insert(key);
            }
            acc
        })
        .reduce(Best::new, merge);

    let mut actions: Vec<(Effect, (u32, usize, usize))> = best.into_iter().collect();
    actions.sort_by(|a, b| (a.1.0, &a.0).cmp(&(b.1.0, &b.0)));
    let mut used: Vec<usize> = actions.iter().map(|a| a.1.1).collect();
    used.sort_unstable();
    used.dedup();
    let new_id: HashMap<usize, u32> = used
        .iter()
        .enumerate()
        .map(|(i, &mi)| (mi, i as u32))
        .collect();
    let macros: Vec<MacroEntry> = used
        .iter()
        .enumerate()
        .map(|(i, &mi)| MacroEntry {
            id: i as u32,
            moves: sym::format_seq(&found[mi].moves),
            kind: found[mi].kind.name(kind).to_string(),
            cycle: found[mi].cycle.clone(),
        })
        .collect();
    let mut by_kind: BTreeMap<String, usize> = BTreeMap::new();
    let entries: Vec<ActionEntry> = actions
        .iter()
        .enumerate()
        .map(|(id, (effect, (cost, mi, si)))| {
            let k = effect.classify().map_or(MacroKind::ThreeCycle, |c| c.0);
            *by_kind.entry(k.name(kind).to_string()).or_default() += 1;
            ActionEntry {
                id: id as u32,
                setup: sym::format_seq(&setups[*si]),
                macro_id: new_id[mi],
                cost: *cost,
                perm: effect.perm.clone(),
                ori_delta: effect.ori.clone(),
            }
        })
        .collect();
    let total: u64 = entries.iter().map(|a| u64::from(a.cost)).sum();
    let report = BuildReport {
        kind,
        stats,
        macros_found: found.len(),
        macros_used: macros.len(),
        actions: entries.len(),
        by_kind,
        mean_cost: total as f64 / entries.len().max(1) as f64,
        max_cost: entries.iter().map(|a| a.cost).max().unwrap_or(0),
    };
    let lib = TypeLibrary {
        type_id: kind.type_id().expect("macro kinds have type ids"),
        content: if kind.is_center() { "color" } else { "piece" }.to_string(),
        orientation_mod: m,
        slots: kind.slot_count(),
        layer_refs: LayerRef::for_kind(kind)
            .iter()
            .map(|r| r.name().to_string())
            .collect(),
        macros,
        actions: entries,
    };
    (lib, report)
}

impl Library {
    /// Canonical JSON (CONVENTIONS §7): keys sorted, no whitespace, UTF-8.
    pub fn to_canonical_json(&self) -> String {
        serde_json::to_value(self)
            .and_then(|v| serde_json::to_string(&v))
            .expect("serializable")
    }

    /// sha256 (hex) of the canonical JSON with `sha256` and `generator.git_commit` removed.
    pub fn content_hash(&self) -> String {
        let mut v = serde_json::to_value(self).expect("serializable");
        let obj = v.as_object_mut().expect("object");
        obj.remove("sha256");
        if let Some(g) = obj.get_mut("generator").and_then(|g| g.as_object_mut()) {
            g.remove("git_commit");
        }
        let text = serde_json::to_string(&v).expect("serializable");
        Sha256::digest(text.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    pub fn from_json(text: &str) -> Result<Self, String> {
        let lib: Library = serde_json::from_str(text).map_err(|e| format!("library.json: {e}"))?;
        if lib.library_version != LIBRARY_VERSION {
            return Err(format!(
                "library_version {} but this build expects {LIBRARY_VERSION}",
                lib.library_version
            ));
        }
        let want = lib.content_hash();
        if lib.sha256 != want {
            return Err(format!(
                "sha256 mismatch: file says {}, content is {want}",
                lib.sha256
            ));
        }
        Ok(lib)
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::from_json(&text)
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(path, self.to_canonical_json())
            .map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn get(&self, kind: OrbitKind) -> Option<&TypeLibrary> {
        self.types.get(kind.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small() -> TypeLibrary {
        let kind = OrbitKind::XCenter;
        let cfg = SearchConfig {
            pairs: vec![(3, 1)],
            setup_len: 1,
            ..SearchConfig::default_for(kind)
        };
        let (found, stats) = discover(kind, &cfg);
        expand(kind, &cfg, &found, stats).0
    }

    #[test]
    fn actions_match_their_moves_and_hash_round_trips() {
        let t = small();
        assert!(!t.actions.is_empty());
        let kind = OrbitKind::XCenter;
        let probe = Probe::new(kind, &SearchConfig::default_for(kind));
        for a in t.actions.iter().step_by(37) {
            let moves = t.action_moves(a).unwrap();
            assert_eq!(moves.len() as u32, a.cost);
            assert_eq!(probe.effect_of_seq(&moves), TypeLibrary::effect(a));
        }
        let mut types = BTreeMap::new();
        types.insert("XCenter".to_string(), t);
        let mut lib = Library {
            library_version: LIBRARY_VERSION,
            sha256: String::new(),
            generator: Generator {
                git_commit: "abc".into(),
                search: BTreeMap::new(),
            },
            types,
        };
        lib.sha256 = lib.content_hash();
        let text = lib.to_canonical_json();
        assert!(!text.contains(' ') && !text.contains('\n'));
        let back = Library::from_json(&text).unwrap();
        assert_eq!(back, lib);
        // git_commit is informational: it does not change the hash.
        let mut other = lib.clone();
        other.generator.git_commit = "def".into();
        assert_eq!(other.content_hash(), lib.content_hash());
        // Any content change does.
        let mut bad = lib;
        bad.types.get_mut("XCenter").unwrap().actions[0].cost += 1;
        assert!(Library::from_json(&bad.to_canonical_json()).is_err());
    }

    #[test]
    fn expansion_is_deterministic() {
        assert_eq!(small(), small());
    }
}
