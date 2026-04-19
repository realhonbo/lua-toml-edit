use mlua::{
    AnyUserData, Error as LuaError, Lua, MetaMethod, Result as LuaResult, Table, UserData,
    UserDataMethods, Value as LuaValue,
};
use std::str::FromStr;
use toml_edit_crate::{
    Array, DocumentMut, Formatted, InlineTable, Item, Table as TomlTable, Value,
};

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
        item = get_child_item(item, key)?;
    }
    Some(item)
}

fn get_child_item<'a>(item: &'a Item, key: &str) -> Option<&'a Item> {
    match item {
        Item::ArrayOfTables(_) => item.get(lua_index_to_rust(key)?),
        Item::Value(Value::Array(_)) => item.get(lua_index_to_rust(key)?),
        _ => item.get(key),
    }
}

fn lua_index_to_rust(key: &str) -> Option<usize> {
    let index = key.parse::<usize>().ok()?;
    index.checked_sub(1)
}

fn set_item(doc: &mut DocumentMut, path: &[String], item: Item) -> LuaResult<()> {
    if path.is_empty() {
        return Err(LuaError::external("path must not be empty"));
    }

    let table = doc.as_table_mut();
    let key = &path[0];
    if path.len() == 1 {
        table.insert(key, item);
        return Ok(());
    }

    if !table.contains_key(key) {
        table.insert(key, Item::Table(TomlTable::new()));
    }
    set_child_item(&mut table[key], &path[1..], item)
}

fn set_child_item(current: &mut Item, path: &[String], item: Item) -> LuaResult<()> {
    let key = &path[0];

    if path.len() == 1 {
        return match current {
            Item::Table(table) => {
                table.insert(key, item);
                Ok(())
            }
            Item::ArrayOfTables(_) | Item::Value(Value::Array(_)) => {
                let index = lua_index_to_rust(key).ok_or_else(|| {
                    LuaError::external(format!("path segment '{key}' is not an array index"))
                })?;
                let target = current.get_mut(index).ok_or_else(|| {
                    LuaError::external(format!("path segment '{key}' is out of bounds"))
                })?;
                *target = item;
                Ok(())
            }
            _ => Err(LuaError::external(format!(
                "path segment '{key}' is not a table"
            ))),
        };
    }

    match current {
        Item::Table(table) => {
            if !table.contains_key(key) {
                table.insert(key, Item::Table(TomlTable::new()));
            }
            set_child_item(&mut table[key], &path[1..], item)
        }
        Item::ArrayOfTables(_) | Item::Value(Value::Array(_)) => {
            let index = lua_index_to_rust(key).ok_or_else(|| {
                LuaError::external(format!("path segment '{key}' is not an array index"))
            })?;
            let target = current.get_mut(index).ok_or_else(|| {
                LuaError::external(format!("path segment '{key}' is out of bounds"))
            })?;
            set_child_item(target, &path[1..], item)
        }
        _ => Err(LuaError::external(format!(
            "path segment '{key}' is not a table"
        ))),
    }
}

fn remove_item(doc: &mut DocumentMut, path: &[String]) -> LuaResult<bool> {
    if path.is_empty() {
        return Err(LuaError::external("path must not be empty"));
    }

    let table = doc.as_table_mut();
    let key = &path[0];
    if path.len() == 1 {
        return Ok(table.remove(key).is_some());
    }

    let Some(item) = table.get_mut(key) else {
        return Ok(false);
    };
    remove_child_item(item, &path[1..])
}

fn remove_child_item(current: &mut Item, path: &[String]) -> LuaResult<bool> {
    let key = &path[0];

    if path.len() == 1 {
        return match current {
            Item::Table(table) => Ok(table.remove(key).is_some()),
            Item::ArrayOfTables(array) => {
                let index = lua_index_to_rust(key).ok_or_else(|| {
                    LuaError::external(format!("path segment '{key}' is not an array index"))
                })?;
                if index >= array.len() {
                    return Ok(false);
                }
                array.remove(index);
                Ok(true)
            }
            Item::Value(Value::Array(array)) => {
                let index = lua_index_to_rust(key).ok_or_else(|| {
                    LuaError::external(format!("path segment '{key}' is not an array index"))
                })?;
                if index >= array.len() {
                    return Ok(false);
                }
                array.remove(index);
                Ok(true)
            }
            _ => Ok(false),
        };
    }

    match current {
        Item::Table(table) => {
            let Some(item) = table.get_mut(key) else {
                return Ok(false);
            };
            remove_child_item(item, &path[1..])
        }
        Item::ArrayOfTables(_) | Item::Value(Value::Array(_)) => {
            let index = lua_index_to_rust(key).ok_or_else(|| {
                LuaError::external(format!("path segment '{key}' is not an array index"))
            })?;
            let Some(item) = current.get_mut(index) else {
                return Ok(false);
            };
            remove_child_item(item, &path[1..])
        }
        _ => Ok(false),
    }
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
