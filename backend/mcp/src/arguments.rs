//! Schema-guided repair of common model-side encoding slips, applied before an MCP
//! call reaches the shared business API. It only changes how a value is encoded,
//! never what it means: `null` means "not given", a JSON object or array sent as a
//! string is parsed, numbers and booleans sent as text are converted, a single item
//! where a list is expected becomes a one-item list, and enum values are matched
//! case-insensitively. Anything else is passed through and validated as before.
use serde_json::{Map, Value};

pub(crate) fn repair(schema: &Value, value: Value) -> Value {
    match schema.get("type").and_then(Value::as_str) {
        Some("object") => object(schema, value),
        Some("array") => array(schema, value),
        Some("integer") => integer(value),
        Some("boolean") => boolean(value),
        Some("string") => string(schema, value),
        _ => value,
    }
}

/// Batch tools advertise `items`, but one item's fields sent directly still work.
pub(crate) fn repair_arguments(schema: &Value, value: Value) -> Value {
    match schema.pointer("/properties/items/items") {
        Some(item) if value.get("items").is_none() => repair(item, value),
        _ => repair(schema, value),
    }
}

fn parsed(text: &str) -> Option<Value> {
    let text = text.trim();
    if text.starts_with('{') || text.starts_with('[') {
        serde_json::from_str(text).ok()
    } else {
        None
    }
}

fn object(schema: &Value, value: Value) -> Value {
    let value = match value {
        Value::String(text) => match parsed(&text) {
            Some(object @ Value::Object(_)) => object,
            _ => Value::String(text),
        },
        other => other,
    };
    let Value::Object(map) = value else {
        return value;
    };
    let properties = schema.get("properties");
    let repaired: Map<String, Value> = map
        .into_iter()
        .filter(|(_, item)| !item.is_null())
        .map(|(key, item)| {
            let item = match properties.and_then(|p| p.get(&key)) {
                Some(property) => repair(property, item),
                None => item,
            };
            (key, item)
        })
        .collect();
    Value::Object(repaired)
}

fn array(schema: &Value, value: Value) -> Value {
    let items = match value {
        Value::Array(items) => items,
        Value::String(text) => match parsed(&text) {
            Some(Value::Array(items)) => items,
            Some(single @ Value::Object(_)) => vec![single],
            _ => vec![Value::String(text)],
        },
        Value::Null => return Value::Null,
        single => vec![single],
    };
    let item_schema = schema.get("items").unwrap_or(&Value::Null);
    Value::Array(
        items
            .into_iter()
            .map(|item| repair(item_schema, item))
            .collect(),
    )
}

fn integer(value: Value) -> Value {
    match &value {
        Value::String(text) => text.trim().parse::<i64>().map(Value::from).unwrap_or(value),
        Value::Number(number) if number.as_i64().is_none() => number
            .as_f64()
            .filter(|f| f.fract() == 0.0 && f.abs() < 9.0e15)
            .map(|f| Value::from(f as i64))
            .unwrap_or(value),
        _ => value,
    }
}

fn boolean(value: Value) -> Value {
    match &value {
        Value::String(text) if text.trim().eq_ignore_ascii_case("true") => Value::Bool(true),
        Value::String(text) if text.trim().eq_ignore_ascii_case("false") => Value::Bool(false),
        _ => value,
    }
}

fn string(schema: &Value, value: Value) -> Value {
    let value = match value {
        Value::Number(number) => Value::String(number.to_string()),
        Value::Bool(flag) => Value::String(flag.to_string()),
        other => other,
    };
    let (Value::String(text), Some(allowed)) =
        (&value, schema.get("enum").and_then(Value::as_array))
    else {
        return value;
    };
    allowed
        .iter()
        .filter_map(Value::as_str)
        .find(|option| option.eq_ignore_ascii_case(text.trim()))
        .map(|option| Value::String(option.to_string()))
        .unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema(name: &str) -> Value {
        let tool = crate::catalog::tools()
            .into_iter()
            .find(|tool| tool.name == name)
            .unwrap();
        Value::Object((*tool.input_schema).clone())
    }

    // 模型常见的编码偏差（null 占位、字符串化的数组/数字/布尔、大小写不同的枚举）
    // 应被修正后再校验，而不是让 AI 收到“参数错误”。
    #[test]
    fn common_encoding_slips_are_repaired() {
        let repaired = repair_arguments(
            &schema("update_entries"),
            json!({"items": "[{\"id\":\"e1\",\"expectedRevision\":\"3\",\"title\":null,\"tags\":\"work\",\"groups\":{\"id\":\"g1\",\"token\":\"t\"}}]"}),
        );
        assert_eq!(
            repaired,
            json!({"items":[{"id":"e1","expectedRevision":3,"tags":["work"],"groups":[{"id":"g1","token":"t"}]}]})
        );
        let repaired = repair_arguments(
            &schema("create_entries"),
            json!({"kind":"API","groups":[{"token":"t","account":null}],"database":null,"services":[{"protocol":"OpenAI","url":"https://a.invalid"}]}),
        );
        assert_eq!(
            repaired,
            json!({"kind":"api","groups":[{"token":"t"}],"services":[{"protocol":"openai","url":"https://a.invalid"}]})
        );
        let repaired = repair_arguments(
            &schema("pin_entries"),
            json!({"id":"e1","expectedRevision":2.0,"pinned":"TRUE"}),
        );
        assert_eq!(
            repaired,
            json!({"id":"e1","expectedRevision":2,"pinned":true})
        );
        let repaired = repair_arguments(
            &schema("write_credential"),
            json!({"credentialId":"e1","field":"Password","groupId":null,"target":"{\"type\":\"file\",\"path\":\"C:\\\\a.txt\"}"}),
        );
        assert_eq!(
            repaired,
            json!({"credentialId":"e1","field":"password","target":{"type":"file","path":"C:\\a.txt"}})
        );
    }

    // 无法确定含义的值保持原样，交给业务校验给出具体字段错误。
    #[test]
    fn meaningful_values_are_left_alone() {
        let value = json!({"items":[{"id":"e1","expectedRevision":"three"}]});
        assert_eq!(
            repair_arguments(&schema("delete_entries"), value.clone()),
            value
        );
        let value = json!({"query":"[not json","kind":"telepathy"});
        assert_eq!(
            repair_arguments(&schema("search_entries"), value.clone()),
            value
        );
    }
}
