//! Validate the data and plan YAML files against their JSON Schemas,
//! plus cross-reference checks JSON Schema can't express.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use jsonschema::Registry;
use serde_json::Value;

const BASE: &str = "file:///schemas/";
const PLAN_SCHEMA: &str = "weekly_plan.schema.yaml";

/// Household data files and their schemas, in validation order. They live in data/
/// (created by the first-run interview, git-ignored); seasonal and sources also ship
/// as starting points in defaults/.
pub const TARGETS: [(&str, &str); 4] = [
    ("profile.yaml", "profile.schema.yaml"),
    ("pantry.yaml", "pantry.schema.yaml"),
    ("seasonal.yaml", "seasonal.schema.yaml"),
    ("sources.yaml", "sources.schema.yaml"),
];

#[derive(Debug, Clone, serde::Serialize)]
pub struct Report {
    /// Path relative to the repo root, with forward slashes.
    pub path: String,
    pub errors: Vec<String>,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }
}

pub struct Validator {
    root: PathBuf,
    schemas: HashMap<String, jsonschema::Validator>,
}

/// Parse YAML into JSON values. YAML 1.2 has no timestamp type, so dates stay
/// strings, which is what `format: date` expects.
pub fn load(path: &Path) -> Result<Value, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_yaml_ng::from_str(&text).map_err(|e| e.to_string())
}

impl Validator {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, String> {
        let root = root.into();
        let mut docs = Vec::new();
        let dir = root.join("schemas");
        for entry in std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))? {
            let path = entry.map_err(|e| e.to_string())?.path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if name.ends_with(".schema.yaml") {
                docs.push((name, load(&path).map_err(|e| format!("{}: {e}", path.display()))?));
            }
        }
        let mut registry = Registry::new();
        for (name, doc) in &docs {
            registry = registry.add(format!("{BASE}{name}"), doc.clone()).map_err(|e| e.to_string())?;
        }
        let registry = registry.prepare().map_err(|e| e.to_string())?;
        let mut schemas = HashMap::new();
        for (name, doc) in &docs {
            let v = jsonschema::options()
                .with_base_uri(format!("{BASE}{name}"))
                .with_registry(&registry)
                .should_validate_formats(true)
                .build(doc)
                .map_err(|e| format!("{name}: {e}"))?;
            schemas.insert(name.clone(), v);
        }
        Ok(Self { root, schemas })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Repo-relative path with forward slashes (or the path as given if outside the repo).
    pub fn rel(&self, path: &Path) -> String {
        path.strip_prefix(&self.root).unwrap_or(path).to_string_lossy().replace('\\', "/")
    }

    pub fn schema_for(&self, path: &Path) -> Option<&'static str> {
        let rel = self.rel(path);
        if let Some((_, file)) = rel.rsplit_once('/').filter(|(dir, _)| *dir == "data" || *dir == "defaults") {
            if let Some((_, s)) = TARGETS.iter().find(|(t, _)| *t == file) {
                return Some(s);
            }
        }
        (rel.starts_with("plans/") || rel.contains("weekly_plan")).then_some(PLAN_SCHEMA)
    }

    /// Every file `validate` checks with no arguments. Data files that don't exist yet
    /// (before the first-run interview) are skipped, not reported.
    pub fn all_files(&self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = ["data", "defaults"]
            .iter()
            .flat_map(|dir| TARGETS.iter().map(move |(t, _)| self.root.join(dir).join(t)))
            .filter(|p| p.exists())
            .collect();
        for dir in ["plans", "schemas/examples"] {
            let mut found: Vec<PathBuf> = std::fs::read_dir(self.root.join(dir))
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "yaml"))
                .collect();
            found.sort();
            out.extend(found);
        }
        out
    }

    pub fn validate_file(&self, path: &Path) -> Report {
        match load(path) {
            Ok(doc) => self.validate_value(path, &doc),
            Err(e) => Report { path: self.rel(path), errors: vec![format!("YAML syntax error: {e}")] },
        }
    }

    /// Validate a document as if it lived at `path` (used to check before writing).
    pub fn validate_value(&self, path: &Path, doc: &Value) -> Report {
        let rel = self.rel(path);
        let Some(schema) = self.schema_for(path) else {
            return Report { path: rel.clone(), errors: vec![format!("don't know which schema applies to {rel}")] };
        };
        let mut errors: Vec<String> = self.schemas[schema]
            .iter_errors(doc)
            .map(|e| {
                let loc = e.instance_path().as_str().trim_start_matches('/').to_string();
                format!("{}: {e}", if loc.is_empty() { "<root>" } else { &loc })
            })
            .collect();
        if schema == PLAN_SCHEMA && errors.is_empty() {
            errors = self.check_plan_refs(doc);
        }
        Report { path: rel, errors }
    }

    pub fn validate_all(&self) -> Vec<Report> {
        self.all_files().iter().map(|p| self.validate_file(p)).collect()
    }

    fn check_plan_refs(&self, doc: &Value) -> Vec<String> {
        let mut errs = Vec::new();
        let arr = |v: &Value, k: &str| v.get(k).and_then(Value::as_array).cloned().unwrap_or_default();
        let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(str::to_owned);
        let n = |v: &Value, k: &str| v.get(k).and_then(Value::as_f64).unwrap_or(0.0);

        // The household's sources, or the shipped defaults before the interview created them.
        let sources = load(&self.root.join("data/sources.yaml"))
            .or_else(|_| load(&self.root.join("defaults/sources.yaml")))
            .unwrap_or(Value::Null);
        let src_ids: HashSet<String> = arr(&sources, "sources").iter().filter_map(|x| s(x, "id")).collect();
        let dishes = arr(doc, "dishes");
        let dish_ids: HashSet<String> = dishes.iter().filter_map(|d| s(d, "id")).collect();

        for d in &dishes {
            if let Some(sid) = d.get("source").and_then(|x| s(x, "source_id")) {
                if !src_ids.contains(&sid) {
                    errs.push(format!("dish {}: unknown source_id '{sid}'", s(d, "id").unwrap_or_default()));
                }
            }
        }
        for cs in arr(doc, "cook_sessions") {
            for d in arr(&cs, "dishes").iter().filter_map(Value::as_str) {
                if !dish_ids.contains(d) {
                    errs.push(format!("cook_session {}: unknown dish '{d}'", s(&cs, "id").unwrap_or_default()));
                }
            }
        }
        for m in arr(doc, "other_meals") {
            if let Some(id) = s(&m, "dish_id").filter(|id| !dish_ids.contains(id)) {
                errs.push(format!("other_meals {}: unknown dish '{id}'", s(&m, "meal").unwrap_or_default()));
            }
        }
        for dn in arr(doc, "dinners") {
            if let Some(id) = s(&dn, "dish_id").filter(|id| !dish_ids.contains(id)) {
                errs.push(format!("dinner {}: unknown dish '{id}'", s(&dn, "day").unwrap_or_default()));
            }
        }
        // Campaigns only count if they still run on the delivery date (ISO dates compare as strings).
        let delivery = doc.get("delivery").and_then(|d| s(d, "date")).unwrap_or_default();
        for deal in arr(doc, "sale_focus") {
            let item = s(&deal, "item").unwrap_or_default();
            if s(&deal, "ends").is_some_and(|end| end < delivery) {
                errs.push(format!("sale_focus {item}: campaign ends before delivery ({delivery})"));
            }
            if n(&deal, "price_dkk") >= n(&deal, "regular_price_dkk") {
                errs.push(format!("sale_focus {item}: price_dkk is not below regular_price_dkk"));
            }
        }
        let on_sale: Vec<Value> =
            arr(doc, "shopping_list").into_iter().filter(|i| i.get("on_sale").and_then(Value::as_bool) == Some(true)).collect();
        for i in &on_sale {
            if n(i, "price_dkk") >= n(i, "regular_price_dkk") {
                errs.push(format!("shopping_list {}: on_sale but price_dkk is not below regular_price_dkk", s(i, "item").unwrap_or_default()));
            }
        }
        if let Some(b) = doc.get("budget").filter(|b| b.as_object().is_some_and(|o| !o.is_empty())) {
            if b.get("savings_dkk").is_some() {
                let round2 = |x: f64| (x * 100.0).round() / 100.0;
                let saved = round2(on_sale.iter().map(|i| n(i, "regular_price_dkk") - n(i, "price_dkk")).sum());
                if (saved - n(b, "savings_dkk")).abs() > 0.5 {
                    errs.push(format!("budget: savings_dkk {} != sum over on_sale items ({saved})", n(b, "savings_dkk")));
                }
            }
            let round2 = |x: f64| (x * 100.0).round() / 100.0;
            let (groceries, total, limit) = (n(b, "groceries_dkk"), n(b, "total_dkk"), n(b, "limit_dkk"));
            let sum = round2(groceries + n(b, "delivery_fee_dkk"));
            if (sum - total).abs() > 0.5 {
                errs.push(format!("budget: total_dkk {total} != groceries + delivery ({sum})"));
            }
            if (total <= limit) != b.get("within_budget").and_then(Value::as_bool).unwrap_or(false) {
                errs.push("budget: within_budget flag doesn't match totals".into());
            }
            let items = round2(arr(doc, "shopping_list").iter().map(|i| n(i, "price_dkk")).sum());
            if (items - groceries).abs() > 0.5 {
                errs.push(format!("budget: groceries_dkk {groceries} != sum of shopping_list ({items})"));
            }
        }
        errs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
    }

    #[test]
    fn repo_files_are_valid() {
        let v = Validator::new(root()).unwrap();
        for r in v.validate_all() {
            assert!(r.ok(), "{}: {:?}", r.path, r.errors);
        }
    }

    #[test]
    fn catches_schema_and_cross_ref_errors() {
        let v = Validator::new(root()).unwrap();
        let dir = std::env::temp_dir().join(format!("validator-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let example = std::fs::read_to_string(root().join("schemas/examples/weekly_plan.example.yaml")).unwrap();

        let bad_enum = dir.join("weekly_plan.bad_enum.yaml");
        std::fs::write(&bad_enum, example.replace("status: draft", "status: maybe")).unwrap();
        let r = v.validate_file(&bad_enum);
        assert!(r.errors.iter().any(|e| e.starts_with("status:")), "{:?}", r.errors);

        let bad_ref = dir.join("weekly_plan.bad_ref.yaml");
        std::fs::write(&bad_ref, example.replace("dish_id: lentil_mushroom_ragu}", "dish_id: nope}")).unwrap();
        let r = v.validate_file(&bad_ref);
        assert!(r.errors.iter().any(|e| e.contains("unknown dish 'nope'")), "{:?}", r.errors);

        let bad_date = dir.join("weekly_plan.bad_date.yaml");
        std::fs::write(&bad_date, example.replace("created: 2026-10-09", "created: 2026-13-45")).unwrap();
        let r = v.validate_file(&bad_date);
        assert!(r.errors.iter().any(|e| e.starts_with("created:")), "{:?}", r.errors);

        let expired = dir.join("weekly_plan.expired_sale.yaml");
        std::fs::write(&expired, example.replace("ends: 2026-10-11}", "ends: 2026-10-10}")).unwrap();
        let r = v.validate_file(&expired);
        assert!(r.errors.iter().any(|e| e.contains("campaign ends before delivery")), "{:?}", r.errors);

        let bad_savings = dir.join("weekly_plan.bad_savings.yaml");
        std::fs::write(&bad_savings, example.replace("savings_dkk: 5", "savings_dkk: 50")).unwrap();
        let r = v.validate_file(&bad_savings);
        assert!(r.errors.iter().any(|e| e.contains("savings_dkk")), "{:?}", r.errors);

        let missing_regular = dir.join("weekly_plan.missing_regular.yaml");
        std::fs::write(&missing_regular, example.replace(", on_sale: true, regular_price_dkk: 25}", ", on_sale: true}")).unwrap();
        let r = v.validate_file(&missing_regular);
        assert!(r.errors.iter().any(|e| e.contains("regular_price_dkk")), "{:?}", r.errors);

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
