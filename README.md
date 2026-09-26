# lua-toml-edit

A Lua native module for editing TOML, built with Rust's `toml_edit` and `mlua`.
Read, update, and remove values while retaining the surrounding document's
comments, formatting, and key order. Replaced values may be reformatted and
their attached comments may be lost.

The LuaRocks package name is `lua-toml-edit`; the Lua module name is `toml_edit`.

## Installation

Building requires Rust/Cargo, a native compiler toolchain, LuaRocks 3.x, and a
matching Lua runtime and development headers. Supported Lua versions are
5.1–5.4, with Cargo features also available for LuaJIT.

From a checkout of this repository:

```sh
eval "$(luarocks --lua-version=5.1 path)"
luarocks --lua-version=5.1 make lua-toml-edit-0.2.7-1.rockspec --local
lua5.1 -e 'assert(require("toml_edit"))'
```

Choose the Lua version matching your application. LuaRocks installs the
`luarocks-build-rust-mlua` build backend and selects the corresponding Cargo
feature. The native module must be built for the runtime that loads it.

After this package is published to LuaRocks, it can also be installed with
`luarocks install lua-toml-edit`.

## Usage

```lua
local toml = require("toml_edit")
local doc = toml.parse([[
# Application configuration
port = 8080
tags = ["web", "internal"]

[server]
host = "localhost"
]])

assert(doc:get("server.host") == "localhost")
doc:set("port", 9000)
doc:set("server.host", "0.0.0.0")
doc:set("tags.2", "public")
doc:set({"literal.key", "enabled"}, true)
doc:set("started_at", toml.raw("1979-05-27T07:32:00Z"))
assert(doc:contains("server.host"))
assert(doc:remove("tags.1"))
print(doc:tostring())
```

## API

| Function or method | Behavior |
| --- | --- |
| `toml.parse(source)` | Parse a TOML string into a document; invalid TOML raises a Lua error. |
| `toml.raw(fragment)` | Parse a TOML value for use with `set`, e.g. a datetime, `[]`, or `{}`. |
| `doc:get(path)` | Return a value, or `nil` if the path does not exist. Tables and arrays are returned as copies. |
| `doc:contains(path)` | Return whether a value exists at the path. |
| `doc:set(path, value)` | Set a value and return the document for chaining. Missing intermediate tables are created. |
| `doc:remove(path)` | Return `true` if an entry was removed, otherwise `false`. Removing an array element shifts subsequent indices. |
| `doc:tostring()` | Serialize the document. `tostring(doc)` is equivalent. |

Paths can be dot-separated strings (`"proxies.2.name"`) or sequence tables
(`{"proxies", 2, "name"}`). Array indices start at **1**. Use a path table when
a key contains a literal dot. Empty paths and empty path segments are invalid.
String paths do not interpret TOML quoted-key syntax.

Strings, booleans, and finite numbers can be written directly. Nonempty Lua
sequence tables become TOML arrays; string-keyed tables become TOML tables.
An empty Lua table becomes a TOML table; use `toml.raw("[]")` for an empty array.
`nil` is not a TOML value; use `remove` to delete entries. Datetimes are returned
as strings; use `raw` to write them as TOML datetimes rather than strings.

`set` replaces existing array elements; it does not append or grow arrays.
Inline tables can be read, but nested edits inside them are not supported;
replace the whole inline table with `raw` instead. Deeply nested tables inside
Lua arrays are also restricted; use a raw TOML array when needed. Changes to a
table returned by `get` do not change the document until written back with `set`.

## Development

```sh
make test                              # Lua 5.1 (default `lua` executable)
make test LUA_VERSION=lua52 LUA=lua5.2
make test LUA_VERSION=lua53 LUA=lua5.3
make test LUA_VERSION=lua54 LUA=lua5.4
make test LUA_VERSION=luajit LUA=luajit
cargo fmt --check
luarocks lint lua-toml-edit-0.2.7-1.rockspec
```

The Makefile supports Linux and macOS. Tests explicitly load the freshly built
module from the checkout. Run builds for different Lua versions sequentially,
as they share the output directory.

See [RELEASING.md](RELEASING.md) for the LuaRocks release procedure.

## License

[MIT](LICENSE).
