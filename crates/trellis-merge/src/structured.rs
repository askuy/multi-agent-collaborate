use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use crate::{ConflictReason, MergeResult};

/// Field-level merge of JSON documents; output is pretty-printed with sorted keys.
pub(crate) fn merge_json(base: Option<&[u8]>, ours: &[u8], theirs: &[u8]) -> MergeResult {
    merge_with(
        base,
        ours,
        theirs,
        |bytes| serde_json::from_slice(bytes).ok(),
        |value| {
            let mut out = serde_json::to_vec_pretty(value).ok()?;
            out.push(b'\n');
            Some(out)
        },
    )
}

/// Field-level merge of YAML documents; output is re-serialized YAML.
pub(crate) fn merge_yaml(base: Option<&[u8]>, ours: &[u8], theirs: &[u8]) -> MergeResult {
    merge_with(
        base,
        ours,
        theirs,
        |bytes| serde_yaml::from_slice(bytes).ok(),
        |value| serde_yaml::to_string(value).ok().map(String::into_bytes),
    )
}

fn merge_with(
    base: Option<&[u8]>,
    ours: &[u8],
    theirs: &[u8],
    parse: impl Fn(&[u8]) -> Option<Value>,
    render: impl Fn(&Value) -> Option<Vec<u8>>,
) -> MergeResult {
    let unparseable =
        || MergeResult::conflict(ConflictReason::Unparseable, "document could not be parsed");
    let (Some(ours), Some(theirs)) = (parse(ours), parse(theirs)) else {
        return unparseable();
    };
    let base = match base {
        Some(bytes) => match parse(bytes) {
            Some(value) => value,
            None => return unparseable(),
        },
        None => Value::Null,
    };
    let mut conflicts = Vec::new();
    let merged = merge_value(&base, &ours, &theirs, "", &mut conflicts);
    if !conflicts.is_empty() {
        return MergeResult::conflict(ConflictReason::FieldOverlap, conflicts.join("\n"));
    }
    match render(&merged) {
        Some(bytes) => MergeResult::Clean(Some(bytes)),
        None => unparseable(),
    }
}

fn display_path(path: &str) -> String {
    if path.is_empty() {
        "/".to_owned()
    } else {
        path.to_owned()
    }
}

fn merge_value(
    base: &Value,
    ours: &Value,
    theirs: &Value,
    path: &str,
    conflicts: &mut Vec<String>,
) -> Value {
    if ours == theirs || base == theirs {
        return ours.clone();
    }
    if base == ours {
        return theirs.clone();
    }
    match (ours, theirs) {
        (Value::Object(o), Value::Object(t)) => {
            let empty = Map::new();
            let b = base.as_object().unwrap_or(&empty);
            let keys: BTreeSet<&String> = b.keys().chain(o.keys()).chain(t.keys()).collect();
            let mut out = Map::new();
            for key in keys {
                let child = format!("{path}/{key}");
                if let Some(v) = merge_entry(b.get(key), o.get(key), t.get(key), &child, conflicts)
                {
                    out.insert(key.clone(), v);
                }
            }
            Value::Object(out)
        }
        (Value::Array(o), Value::Array(t))
            if is_keyed(o) && is_keyed(t) && base.as_array().is_none_or(|b| is_keyed(b)) =>
        {
            let b = base.as_array().map(Vec::as_slice).unwrap_or_default();
            Value::Array(merge_keyed(b, o, t, path, conflicts))
        }
        _ => {
            conflicts.push(display_path(path));
            ours.clone()
        }
    }
}

fn merge_entry(
    base: Option<&Value>,
    ours: Option<&Value>,
    theirs: Option<&Value>,
    path: &str,
    conflicts: &mut Vec<String>,
) -> Option<Value> {
    if ours == theirs || base == theirs {
        return ours.cloned();
    }
    if base == ours {
        return theirs.cloned();
    }
    match (ours, theirs) {
        (Some(o), Some(t)) => Some(merge_value(
            base.unwrap_or(&Value::Null),
            o,
            t,
            path,
            conflicts,
        )),
        _ => {
            conflicts.push(format!(
                "{} (modified on one side, deleted on the other)",
                display_path(path)
            ));
            ours.cloned()
        }
    }
}

fn item_id(value: &Value) -> Option<&str> {
    value.get("id")?.as_str()
}

/// Every element is an object with a unique string `id`.
fn is_keyed(items: &[Value]) -> bool {
    let mut seen = BTreeSet::new();
    items
        .iter()
        .all(|v| item_id(v).is_some_and(|id| seen.insert(id)))
}

fn by_id(items: &[Value]) -> BTreeMap<&str, &Value> {
    items
        .iter()
        .filter_map(|v| Some((item_id(v)?, v)))
        .collect()
}

/// Merges arrays item by item, matched by `id`: ours' order first, then items only theirs added.
fn merge_keyed(
    base: &[Value],
    ours: &[Value],
    theirs: &[Value],
    path: &str,
    conflicts: &mut Vec<String>,
) -> Vec<Value> {
    let (b, o, t) = (by_id(base), by_id(ours), by_id(theirs));
    let mut order: Vec<&str> = ours.iter().filter_map(item_id).collect();
    order.extend(
        theirs
            .iter()
            .filter_map(item_id)
            .filter(|id| !o.contains_key(id)),
    );
    order
        .into_iter()
        .filter_map(|id| {
            let child = format!("{path}[id={id}]");
            merge_entry(
                b.get(id).copied(),
                o.get(id).copied(),
                t.get(id).copied(),
                &child,
                conflicts,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use serde_json::{Value, json};
    use trellis_core::FileKind;

    use crate::{Conflict, merge3};

    use super::*;

    fn run(base: Option<Value>, ours: Value, theirs: Value) -> MergeResult {
        let enc = |v: &Value| serde_json::to_vec(v).unwrap();
        merge3(
            FileKind::Json,
            base.as_ref().map(enc).as_deref(),
            Some(enc(&ours).as_slice()),
            Some(enc(&theirs).as_slice()),
        )
    }

    fn clean_value(r: &MergeResult) -> Value {
        match r {
            MergeResult::Clean(Some(bytes)) => serde_json::from_slice(bytes).unwrap(),
            other => panic!("expected a clean merge, got {other:?}"),
        }
    }

    fn conflict_detail(r: &MergeResult) -> String {
        match r {
            MergeResult::Conflict(Conflict {
                reason: ConflictReason::FieldOverlap,
                detail,
            }) => detail.clone(),
            other => panic!("expected a field conflict, got {other:?}"),
        }
    }

    #[test]
    fn disjoint_fields_merge() {
        let r = run(
            Some(json!({"a": 1, "b": 1})),
            json!({"a": 2, "b": 1}),
            json!({"a": 1, "b": 2}),
        );
        assert_eq!(clean_value(&r), json!({"a": 2, "b": 2}));
    }

    #[test]
    fn nested_disjoint_fields_merge() {
        let base = json!({"alice": {"hp": 3, "place": "inn"}});
        let r = run(
            Some(base),
            json!({"alice": {"hp": 2, "place": "inn"}}),
            json!({"alice": {"hp": 3, "place": "road"}}),
        );
        assert_eq!(
            clean_value(&r),
            json!({"alice": {"hp": 2, "place": "road"}})
        );
    }

    #[test]
    fn same_field_changed_differently_conflicts_with_its_path() {
        let r = run(
            Some(json!({"a": {"b": 1}})),
            json!({"a": {"b": 2}}),
            json!({"a": {"b": 3}}),
        );
        assert!(conflict_detail(&r).contains("/a/b"));
    }

    #[test]
    fn modify_delete_inside_a_document_conflicts() {
        let r = run(
            Some(json!({"a": 1, "b": 1})),
            json!({"b": 1}),
            json!({"a": 5, "b": 1}),
        );
        assert!(conflict_detail(&r).contains("/a (modified on one side, deleted on the other)"));
    }

    #[test]
    fn arrays_of_objects_with_ids_merge_by_id() {
        let base = json!({"chars": [{"id": "alice", "hp": 3}, {"id": "bob", "hp": 3}]});
        let ours = json!({"chars": [{"id": "alice", "hp": 2}, {"id": "bob", "hp": 3}]});
        let theirs = json!({"chars": [{"id": "alice", "hp": 3}, {"id": "bob", "hp": 1}, {"id": "carol", "hp": 5}]});
        let r = run(Some(base), ours, theirs);
        assert_eq!(
            clean_value(&r),
            json!({"chars": [{"id": "alice", "hp": 2}, {"id": "bob", "hp": 1}, {"id": "carol", "hp": 5}]})
        );
    }

    #[test]
    fn deleting_an_id_item_on_one_side_merges() {
        let base = json!([{"id": "a"}, {"id": "b"}]);
        let r = run(
            Some(base),
            json!([{"id": "a"}]),
            json!([{"id": "a"}, {"id": "b"}, {"id": "c"}]),
        );
        assert_eq!(clean_value(&r), json!([{"id": "a"}, {"id": "c"}]));
    }

    #[test]
    fn plain_arrays_changed_on_both_sides_conflict() {
        let r = run(
            Some(json!({"tags": [1]})),
            json!({"tags": [1, 2]}),
            json!({"tags": [1, 3]}),
        );
        assert!(conflict_detail(&r).contains("/tags"));
    }

    #[test]
    fn both_added_documents_merge_disjoint_keys() {
        let r = run(None, json!({"a": 1}), json!({"b": 2}));
        assert_eq!(clean_value(&r), json!({"a": 1, "b": 2}));
    }

    #[test]
    fn unparseable_json_conflicts() {
        let r = merge3(FileKind::Json, Some(b"{}"), Some(b"{"), Some(b"{\"a\":1}"));
        assert!(matches!(
            r,
            MergeResult::Conflict(Conflict {
                reason: ConflictReason::Unparseable,
                ..
            })
        ));
    }

    #[test]
    fn yaml_merges_by_field() {
        let r = merge3(
            FileKind::Yaml,
            Some(b"a: 1\nb: 1\n"),
            Some(b"a: 2\nb: 1\n"),
            Some(b"a: 1\nb: 3\n"),
        );
        let MergeResult::Clean(Some(bytes)) = r else {
            panic!("expected a clean merge, got {r:?}")
        };
        let merged: Value = serde_yaml::from_slice(&bytes).unwrap();
        assert_eq!(merged, json!({"a": 2, "b": 3}));
    }

    proptest! {
        #[test]
        fn edits_to_disjoint_keys_always_merge(
            base in prop::collection::btree_map("[a-h]", 0i64..100, 1..8),
            ours_new in 0i64..100,
            theirs_new in 0i64..100,
        ) {
            let keys: Vec<String> = base.keys().cloned().collect();
            let mut ours = base.clone();
            let mut theirs = base.clone();
            ours.insert(keys[0].clone(), ours_new);
            theirs.insert("z".to_owned(), theirs_new);
            let r = run(Some(json!(base)), json!(ours), json!(theirs));
            let mut expected = ours.clone();
            expected.insert("z".to_owned(), theirs_new);
            prop_assert_eq!(clean_value(&r), json!(expected));
        }
    }
}
