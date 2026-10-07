//! Minimal RFC-6902 (JSON Patch) applier for `STATE_DELTA`.

use serde_json::Value;

use super::types::PatchOp;

/// Applies one operation to `doc`. On error the document is left unchanged for that operation.
pub fn apply(doc: &mut Value, op: &PatchOp) -> Result<(), String> {
    match op.op.as_str() {
        "add" => add(doc, &op.path, op.value.clone()),
        "replace" => {
            if op.path.is_empty() {
                *doc = op.value.clone();
                return Ok(());
            }
            let slot = doc
                .pointer_mut(&op.path)
                .ok_or_else(|| format!("replace: no value at {}", op.path))?;
            *slot = op.value.clone();
            Ok(())
        }
        "remove" => remove(doc, &op.path).map(|_| ()),
        "move" => {
            let value = remove(doc, &op.from)?;
            add(doc, &op.path, value)
        }
        "copy" => {
            let value = doc
                .pointer(&op.from)
                .cloned()
                .ok_or_else(|| format!("copy: no value at {}", op.from))?;
            add(doc, &op.path, value)
        }
        // Assertions carry no state change; a failing one is the server's problem, not ours.
        "test" => Ok(()),
        other => Err(format!("unsupported patch op {other:?}")),
    }
}

fn split(path: &str) -> Result<(&str, String), String> {
    let idx = path
        .rfind('/')
        .ok_or_else(|| format!("invalid pointer {path:?}"))?;
    let key = path[idx + 1..].replace("~1", "/").replace("~0", "~");
    Ok((&path[..idx], key))
}

fn add(doc: &mut Value, path: &str, value: Value) -> Result<(), String> {
    if path.is_empty() {
        *doc = value;
        return Ok(());
    }
    let (parent_path, key) = split(path)?;
    let parent = doc
        .pointer_mut(parent_path)
        .ok_or_else(|| format!("add: no parent at {parent_path:?}"))?;
    match parent {
        Value::Object(map) => {
            map.insert(key, value);
            Ok(())
        }
        Value::Array(items) => {
            let index = if key == "-" {
                items.len()
            } else {
                key.parse::<usize>()
                    .map_err(|_| format!("add: bad array index {key:?}"))?
            };
            if index > items.len() {
                return Err(format!("add: index {index} out of range"));
            }
            items.insert(index, value);
            Ok(())
        }
        _ => Err(format!("add: parent of {path} is not a container")),
    }
}

fn remove(doc: &mut Value, path: &str) -> Result<Value, String> {
    if path.is_empty() {
        return Err("remove: cannot remove the document root".to_owned());
    }
    let (parent_path, key) = split(path)?;
    let parent = doc
        .pointer_mut(parent_path)
        .ok_or_else(|| format!("remove: no parent at {parent_path:?}"))?;
    match parent {
        Value::Object(map) => map
            .remove(&key)
            .ok_or_else(|| format!("remove: no member {key:?}")),
        Value::Array(items) => {
            let index = key
                .parse::<usize>()
                .map_err(|_| format!("remove: bad array index {key:?}"))?;
            if index >= items.len() {
                return Err(format!("remove: index {index} out of range"));
            }
            Ok(items.remove(index))
        }
        _ => Err(format!("remove: parent of {path} is not a container")),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn op(op: &str, path: &str, value: Value, from: &str) -> PatchOp {
        PatchOp {
            op: op.into(),
            path: path.into(),
            value,
            from: from.into(),
        }
    }

    #[test]
    fn add_replace_remove_move_copy() {
        let mut doc = json!({"todos": [{"id": 1}], "a/b": 1});
        apply(&mut doc, &op("add", "/todos/-", json!({"id": 2}), "")).unwrap();
        apply(&mut doc, &op("add", "/todos/0", json!({"id": 0}), "")).unwrap();
        apply(&mut doc, &op("replace", "/a~1b", json!(2), "")).unwrap();
        apply(&mut doc, &op("remove", "/todos/1", Value::Null, "")).unwrap();
        apply(&mut doc, &op("copy", "/c", Value::Null, "/a~1b")).unwrap();
        apply(&mut doc, &op("move", "/d", Value::Null, "/c")).unwrap();
        assert_eq!(
            doc,
            json!({"todos": [{"id": 0}, {"id": 2}], "a/b": 2, "d": 2})
        );
    }

    #[test]
    fn errors_leave_the_document_alone() {
        let mut doc = json!({"a": 1});
        assert!(apply(&mut doc, &op("remove", "/zz", Value::Null, "")).is_err());
        assert!(apply(&mut doc, &op("frobnicate", "/a", Value::Null, "")).is_err());
        assert_eq!(doc, json!({"a": 1}));
    }
}
