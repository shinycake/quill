//! Panic-free access to a `serde_json::Value`. `value["key"] = ..` panics
//! when the value is an array, string or number, and clippy's
//! `indexing_slicing` flags every index into a `Value`. [`set`] and
//! [`set_path`] write only when the value is an object (or `Null`, which
//! `IndexMut` also turns into an object) and leave anything else untouched;
//! [`JsonExt::at`] and [`JsonExt::nth`] read like `value["key"]` and
//! `value[0]`: a missing entry is `Null`.

#![deny(clippy::indexing_slicing)]

use serde_json::{Map, Value};

static NULL: Value = Value::Null;

/// Reads that behave like `Value`'s `Index` impls, without indexing syntax.
pub trait JsonExt {
    /// `self[key]`: the field of an object, `Null` when there is none.
    fn at(&self, key: &str) -> &Value;
    /// `self[index]`: the item of an array, `Null` when there is none.
    fn nth(&self, index: usize) -> &Value;
}

impl JsonExt for Value {
    fn at(&self, key: &str) -> &Value {
        self.get(key).unwrap_or(&NULL)
    }

    fn nth(&self, index: usize) -> &Value {
        self.get(index).unwrap_or(&NULL)
    }
}

/// The object a write goes into: `Null` becomes an empty object first, any
/// other non-object yields `None`.
fn object_mut(value: &mut Value) -> Option<&mut Map<String, Value>> {
    if value.is_null() {
        *value = Value::Object(Map::new());
    }
    value.as_object_mut()
}

/// `value[key] = new`, skipped when `value` is not an object.
pub fn set(value: &mut Value, key: &str, new: Value) {
    if let Some(map) = object_mut(value) {
        map.insert(key.to_string(), new);
    }
}

/// `value[path[0]][path[1]]... = new`, creating missing objects on the way
/// and skipped when something on the path is not an object.
pub fn set_path(value: &mut Value, path: &[&str], new: Value) {
    let Some((last, parents)) = path.split_last() else {
        return;
    };
    let mut cur = value;
    for key in parents {
        let Some(map) = object_mut(cur) else {
            return;
        };
        cur = map.entry((*key).to_string()).or_insert(Value::Null);
    }
    set(cur, last, new);
}

#[cfg(test)]
mod tests {
    use super::{JsonExt, set, set_path};
    use serde_json::{Value, json};

    #[test]
    fn reads_default_to_null() {
        let v = json!({ "a": { "b": [7] } });
        assert_eq!(v.at("a").at("b").nth(0), 7);
        assert!(v.at("x").at("y").nth(3).is_null());
        assert!(json!(5).at("a").is_null());
    }

    #[test]
    fn sets_on_objects_and_null() {
        let mut v = json!({ "a": 1 });
        set(&mut v, "b", json!(2));
        assert_eq!(v, json!({ "a": 1, "b": 2 }));
        let mut n = Value::Null;
        set(&mut n, "k", json!(true));
        assert_eq!(n, json!({ "k": true }));
    }

    #[test]
    fn skips_non_objects() {
        let mut v = json!([1, 2]);
        set(&mut v, "k", json!(1));
        assert_eq!(v, json!([1, 2]));
        let mut s = json!({ "video": "x" });
        set_path(&mut s, &["video", "cover"], json!(1));
        assert_eq!(s, json!({ "video": "x" }));
    }

    #[test]
    fn path_creates_missing_objects() {
        let mut v = json!({});
        set_path(&mut v, &["video", "cover"], json!("c"));
        assert_eq!(v, json!({ "video": { "cover": "c" } }));
        set_path(&mut v, &[], json!(1));
        assert_eq!(v, json!({ "video": { "cover": "c" } }));
    }
}
