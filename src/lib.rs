use mlua::{
    AnyUserData, Error as LuaError, Lua, MetaMethod, Result as LuaResult, Table, UserData,
    UserDataMethods, Value as LuaValue,
};
use std::str::FromStr;
use toml_edit::{Array, DocumentMut, Formatted, InlineTable, Item, Table as TomlTable, Value};

#[derive(Clone)]
struct RawTomlValue {
    item: Item,
}

impl UserData for RawTomlValue {}

struct Document {
    doc: DocumentMut,
}

impl UserData for Document {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("get", |lua, this, path: LuaValue| {
            let path = parse_path(path)?;
            match get_item(&this.doc, &path) {
                Some(item) => item_to_lua(lua, item),
                None => Ok(LuaValue::Nil),
            }
        });

        methods.add_function_mut(
            "set",
            |_, (this, path, value): (AnyUserData, LuaValue, LuaValue)| {
                let path = parse_path(path)?;
                let item = lua_to_item(value)?;
                {
                    let mut doc = this.borrow_mut::<Document>()?;
                    set_item(&mut doc.doc, &path, item)?;
                }
                Ok(this)
            },
        );

        methods.add_method_mut("remove", |_, this, path: LuaValue| {
            let path = parse_path(path)?;
            remove_item(&mut this.doc, &path)
        });

        methods.add_method("contains", |_, this, path: LuaValue| {
            let path = parse_path(path)?;
            Ok(get_item(&this.doc, &path).is_some())
        });

        methods.add_method("tostring", |_, this, ()| Ok(this.doc.to_string()));
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| Ok(this.doc.to_string()));
    }
}

#[mlua::lua_module]
fn toml_edit(lua: &Lua) -> LuaResult<Table> {
    let exports = lua.create_table()?;

    exports.set(
        "parse",
        lua.create_function(|_, source: String| {
            let doc = DocumentMut::from_str(&source).map_err(external_err)?;
            Ok(Document { doc })
        })?,
    )?;

    exports.set(
        "raw",
        lua.create_function(|_, fragment: String| {
            let item = parse_raw_value(&fragment)?;
            Ok(RawTomlValue { item })
        })?,
    )?;

    Ok(exports)
}

fn parse_path(value: LuaValue) -> LuaResult<Vec<String>> {
    match value {
        LuaValue::String(path) => {
            let path = path.to_str()?;
            if path.is_empty() {
                return Err(LuaError::external("path must not be empty"));
            }
            let parts: Vec<String> = path.split('.').map(str::to_owned).collect();
            if parts.iter().any(|part| part.is_empty()) {
                return Err(LuaError::external("path must not contain empty segments"));
            }
            Ok(parts)
        }
        LuaValue::Table(table) => {
            let len = table.raw_len();
            if len == 0 {
                return Err(LuaError::external("path array must not be empty"));
            }

            let mut parts = Vec::with_capacity(len);
            for index in 1..=len {
                let key: String = table.raw_get(index)?;
                if key.is_empty() {
                    return Err(LuaError::external("path must not contain empty segments"));
                }
                parts.push(key);
            }
            Ok(parts)
        }
        _ => Err(LuaError::external("path must be a string or array table")),
    }
}

fn get_item<'a>(doc: &'a DocumentMut, path: &[String]) -> Option<&'a Item> {
    if path.is_empty() {
        return None;
    }

    let mut item = doc.get(&path[0])?;
    for key in &path[1..] {
        item = item.get(key)?;
    }
    Some(item)
}

fn set_item(doc: &mut DocumentMut, path: &[String], item: Item) -> LuaResult<()> {
    if path.is_empty() {
        return Err(LuaError::external("path must not be empty"));
    }

    let mut table = doc.as_table_mut();
    for key in &path[..path.len() - 1] {
        if !table.contains_key(key) {
            table.insert(key, Item::Table(TomlTable::new()));
        }
        table = table[key]
            .as_table_mut()
            .ok_or_else(|| LuaError::external(format!("path segment '{key}' is not a table")))?;
    }

    table.insert(path.last().expect("non-empty path"), item);
    Ok(())
}

fn remove_item(doc: &mut DocumentMut, path: &[String]) -> LuaResult<bool> {
    if path.is_empty() {
        return Err(LuaError::external("path must not be empty"));
    }

    let mut table = doc.as_table_mut();
    for key in &path[..path.len() - 1] {
        match table.get_mut(key).and_then(Item::as_table_mut) {
            Some(next) => table = next,
            None => return Ok(false),
        }
    }

    Ok(table.remove(path.last().expect("non-empty path")).is_some())
}

fn lua_to_item(value: LuaValue) -> LuaResult<Item> {
    match value {
        LuaValue::Nil => Err(LuaError::external("nil cannot be written as a TOML value")),
        LuaValue::Boolean(value) => Ok(Item::Value(Value::Boolean(Formatted::new(value)))),
        LuaValue::Integer(value) => Ok(Item::Value(Value::Integer(Formatted::new(value)))),
        LuaValue::Number(value) => {
            if !value.is_finite() {
                return Err(LuaError::external(
                    "non-finite numbers are not valid TOML values",
                ));
            }

            if value.fract() == 0.0 && value >= i64::MIN as f64 && value <= i64::MAX as f64 {
                Ok(Item::Value(Value::Integer(Formatted::new(value as i64))))
            } else {
                Ok(Item::Value(Value::Float(Formatted::new(value))))
            }
        }
        LuaValue::String(value) => Ok(Item::Value(Value::String(Formatted::new(
            value.to_str()?.to_owned(),
        )))),
        LuaValue::Table(table) => table_to_item(table),
        LuaValue::UserData(userdata) => {
            if let Ok(raw) = userdata.borrow::<RawTomlValue>() {
                Ok(raw.item.clone())
            } else {
                Err(LuaError::external("unsupported userdata TOML value"))
            }
        }
        _ => Err(LuaError::external("unsupported TOML value type")),
    }
}

fn table_to_item(table: Table) -> LuaResult<Item> {
    if is_array_table(&table)? {
        let mut array = Array::new();
        for index in 1..=table.raw_len() {
            let value = lua_to_value(table.raw_get(index)?)?;
            array.push(value);
        }
        Ok(Item::Value(Value::Array(array)))
    } else {
        let mut toml_table = TomlTable::new();
        for pair in table.pairs::<LuaValue, LuaValue>() {
            let (key, value) = pair?;
            let key = match key {
                LuaValue::String(key) => key.to_str()?.to_owned(),
                _ => return Err(LuaError::external("TOML table keys must be strings")),
            };
            toml_table.insert(&key, lua_to_item(value)?);
        }
        Ok(Item::Table(toml_table))
    }
}

fn lua_to_value(value: LuaValue) -> LuaResult<Value> {
    match lua_to_item(value)? {
        Item::Value(value) => Ok(value),
        Item::Table(table) => {
            let mut inline = InlineTable::new();
            for (key, value) in table.iter() {
                let value = value
                    .as_value()
                    .cloned()
                    .ok_or_else(|| LuaError::external("arrays cannot contain TOML tables"))?;
                inline.insert(key, value);
            }
            Ok(Value::InlineTable(inline))
        }
        _ => Err(LuaError::external("unsupported array value")),
    }
}

fn is_array_table(table: &Table) -> LuaResult<bool> {
    let len = table.raw_len();
    if len == 0 {
        return Ok(false);
    }

    let mut count = 0usize;
    for pair in table.clone().pairs::<LuaValue, LuaValue>() {
        let (key, _) = pair?;
        match key {
            LuaValue::Integer(index) if index >= 1 && index as usize <= len => count += 1,
            _ => return Ok(false),
        }
    }

    Ok(count == len)
}

fn item_to_lua(lua: &Lua, item: &Item) -> LuaResult<LuaValue> {
    match item {
        Item::None => Ok(LuaValue::Nil),
        Item::Value(value) => value_to_lua(lua, value),
        Item::Table(table) => {
            let lua_table = lua.create_table()?;
            for (key, item) in table.iter() {
                lua_table.set(key, item_to_lua(lua, item)?)?;
            }
            Ok(LuaValue::Table(lua_table))
        }
        Item::ArrayOfTables(array) => {
            let lua_array = lua.create_table()?;
            for (index, table) in array.iter().enumerate() {
                let lua_table = lua.create_table()?;
                for (key, item) in table.iter() {
                    lua_table.set(key, item_to_lua(lua, item)?)?;
                }
                lua_array.raw_set(index + 1, lua_table)?;
            }
            Ok(LuaValue::Table(lua_array))
        }
    }
}

fn value_to_lua(lua: &Lua, value: &Value) -> LuaResult<LuaValue> {
    match value {
        Value::String(value) => Ok(LuaValue::String(lua.create_string(value.value())?)),
        Value::Integer(value) => Ok(LuaValue::Integer(*value.value())),
        Value::Float(value) => Ok(LuaValue::Number(*value.value())),
        Value::Boolean(value) => Ok(LuaValue::Boolean(*value.value())),
        Value::Datetime(value) => Ok(LuaValue::String(
            lua.create_string(value.value().to_string())?,
        )),
        Value::Array(array) => {
            let lua_array = lua.create_table()?;
            for (index, value) in array.iter().enumerate() {
                lua_array.raw_set(index + 1, value_to_lua(lua, value)?)?;
            }
            Ok(LuaValue::Table(lua_array))
        }
        Value::InlineTable(table) => {
            let lua_table = lua.create_table()?;
            for (key, value) in table.iter() {
                lua_table.set(key, value_to_lua(lua, value)?)?;
            }
            Ok(LuaValue::Table(lua_table))
        }
    }
}

fn parse_raw_value(fragment: &str) -> LuaResult<Item> {
    let wrapped = format!("value = {fragment}");
    let doc = DocumentMut::from_str(&wrapped).map_err(external_err)?;
    doc.get("value")
        .cloned()
        .ok_or_else(|| LuaError::external("raw TOML fragment did not produce a value"))
}

fn external_err(error: impl std::error::Error + Send + Sync + 'static) -> LuaError {
    LuaError::external(error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_value_without_losing_comment() {
        let source = r#"
# keep me
[server]
host = "127.0.0.1"
port = 8080
"#;
        let mut doc = DocumentMut::from_str(source).unwrap();

        set_item(
            &mut doc,
            &["server".to_owned(), "port".to_owned()],
            Item::Value(Value::Integer(Formatted::new(9000))),
        )
        .unwrap();

        let out = doc.to_string();
        assert!(out.contains("# keep me"));
        assert!(out.contains("host = \"127.0.0.1\""));
        assert!(out.contains("port = 9000"));
    }

    #[test]
    fn creates_nested_table_with_literal_dot_key() {
        let mut doc = DocumentMut::new();

        set_item(
            &mut doc,
            &["a.b".to_owned(), "c".to_owned()],
            Item::Value(Value::String(Formatted::new("value".to_owned()))),
        )
        .unwrap();

        assert_eq!(
            get_item(&doc, &["a.b".to_owned(), "c".to_owned()])
                .unwrap()
                .as_str(),
            Some("value")
        );
        let out = doc.to_string();
        assert!(out.contains("[\"a.b\"]"));
        assert!(out.contains("c = \"value\""));
    }

    #[test]
    fn parses_raw_datetime_fragment() {
        let item = parse_raw_value("1979-05-27T07:32:00Z").unwrap();
        assert!(matches!(item, Item::Value(Value::Datetime(_))));
    }

    #[test]
    fn refuses_to_replace_path_prefix_scalar_with_table() {
        let mut doc = DocumentMut::from_str("a = 1\n").unwrap();
        let err = set_item(
            &mut doc,
            &["a".to_owned(), "b".to_owned()],
            Item::Value(Value::Integer(Formatted::new(2))),
        )
        .unwrap_err();

        assert!(err.to_string().contains("path segment 'a' is not a table"));
        assert!(doc.to_string().contains("a = 1"));
    }
}
