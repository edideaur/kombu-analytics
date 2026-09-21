#![forbid(unsafe_code)]

use crate::constants::{
    DATA_TYPE_ARRAY, DATA_TYPE_BOOLEAN, DATA_TYPE_DATE, DATA_TYPE_NUMBER, DATA_TYPE_STRING,
    EVENT_DATA_MAX_KEYS, FIELD_LENGTH_STRING_VALUE,
};

#[derive(Debug, Clone)]
pub struct KeyValueData {
    pub key: String,
    pub value: String,
    pub data_type: i32,
}

fn is_valid_date_value(value: &str) -> bool {
    value.len() >= 19
        && value.as_bytes()[4] == b'-'
        && value.as_bytes()[7] == b'-'
        && value.contains('T')
}

fn get_data_type(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Number(_) => "number",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::String(s) if is_valid_date_value(s) => "date",
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => "object",
        serde_json::Value::String(_) | serde_json::Value::Null => "string",
    }
}

fn truncate_string(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect()
    }
}

pub fn create_key_value(key: String, value: &serde_json::Value) -> KeyValueData {
    let t = get_data_type(value);
    let (data_type, processed) = match t {
        "number" => (DATA_TYPE_NUMBER, value.to_string()),
        "boolean" => (
            DATA_TYPE_BOOLEAN,
            if value.as_bool().unwrap_or(false) {
                "true".to_string()
            } else {
                "false".to_string()
            },
        ),
        "date" => (DATA_TYPE_DATE, value.as_str().unwrap_or("").to_string()),
        "object" => (DATA_TYPE_ARRAY, value.to_string()),
        _ => (
            DATA_TYPE_STRING,
            value.as_str().unwrap_or(&value.to_string()).to_string(),
        ),
    };
    let truncated = truncate_string(&processed, FIELD_LENGTH_STRING_VALUE);
    KeyValueData {
        key,
        value: truncated,
        data_type,
    }
}

pub fn flatten_json(obj: &serde_json::Value, parent: &str, out: &mut Vec<KeyValueData>) {
    if let serde_json::Value::Object(map) = obj {
        for (k, v) in map {
            let full = if parent.is_empty() {
                k.clone()
            } else {
                format!("{parent}.{k}")
            };
            if v.is_object() && !is_valid_date_value(v.as_str().unwrap_or("")) {
                flatten_json(v, &full, out);
            } else {
                out.push(create_key_value(full, v));
            }
        }
    }
}

pub fn flatten_event_data(data: &serde_json::Value) -> Result<Vec<KeyValueData>, String> {
    let mut out = Vec::new();
    flatten_json(data, "", &mut out);
    if out.len() > EVENT_DATA_MAX_KEYS {
        return Err(format!(
            "too many keys: {} > {}",
            out.len(),
            EVENT_DATA_MAX_KEYS
        ));
    }
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn flatten_limit() {
        let mut obj = serde_json::Map::new();
        for i in 0..51 {
            obj.insert(format!("k{i}"), json!("v"));
        }
        assert!(flatten_event_data(&json!(obj)).is_err());
    }

    #[test]
    fn flatten_ok() {
        let data = json!({"a": 1, "b": "hello", "c": true});
        let v = flatten_event_data(&data).unwrap();
        assert_eq!(v.len(), 3);
    }

    #[test]
    fn test_types_and_nesting() {
        let data = json!({
            "num": 42.5,
            "bool_t": true,
            "bool_f": false,
            "date": "2024-06-15T12:00:00Z",
            "arr": [1, 2, 3],
            "nested": {
                "inner": "value"
            }
        });

        let flattened = flatten_event_data(&data).unwrap();
        assert_eq!(flattened.len(), 6);
        let nested_item = flattened
            .iter()
            .find(|kv| kv.key == "nested.inner")
            .unwrap();
        assert_eq!(nested_item.value, "value");
        assert_eq!(nested_item.data_type, DATA_TYPE_STRING);

        let date_item = flattened.iter().find(|kv| kv.key == "date").unwrap();
        assert_eq!(date_item.data_type, DATA_TYPE_DATE);

        let num_item = flattened.iter().find(|kv| kv.key == "num").unwrap();
        assert_eq!(num_item.data_type, DATA_TYPE_NUMBER);
    }

    #[test]
    fn test_non_object_and_null_and_truncation() {
        let mut out = Vec::new();
        flatten_json(&json!(42), "", &mut out);
        assert!(out.is_empty());
        flatten_json(&json!(null), "", &mut out);
        assert!(out.is_empty());
        flatten_json(&json!([1, 2]), "", &mut out);
        assert!(out.is_empty());
        let v = flatten_event_data(&json!("just a string")).unwrap();
        assert!(out.is_empty());
        assert!(v.is_empty());

        let data_null = json!({"n": null});
        let vnull = flatten_event_data(&data_null).unwrap();
        assert_eq!(vnull.len(), 1);
        assert_eq!(vnull[0].data_type, DATA_TYPE_STRING);

        let long = "x".repeat(600);
        let kv = create_key_value("k".into(), &json!(long));
        assert_eq!(kv.value.len(), 500);

        assert!(!super::is_valid_date_value("short"));
        assert!(!super::is_valid_date_value("1234X56-78T12:00:00ZZZ"));
        assert!(!super::is_valid_date_value("2024-06-15 12:00:00ZZZ"));
        assert!(!super::is_valid_date_value("2024/06/15T12:00:00Z___"));
        assert!(super::is_valid_date_value("2024-06-15T12:00:00Z"));
    }
}

#[cfg(kani)]
mod kani_proofs {
    #[kani::proof]
    fn harness_keys_bound() {
        kani::assert(super::EVENT_DATA_MAX_KEYS == 50, "max keys 50");
    }

    #[kani::proof]
    fn harness_data_types_bound() {
        let choice: u8 = kani::any();
        let dt = match choice % 5 {
            0 => super::DATA_TYPE_STRING,
            1 => super::DATA_TYPE_NUMBER,
            2 => super::DATA_TYPE_BOOLEAN,
            3 => super::DATA_TYPE_DATE,
            _ => super::DATA_TYPE_ARRAY,
        };
        kani::assert(dt >= 1 && dt <= 5, "valid data type in range");
    }
}
