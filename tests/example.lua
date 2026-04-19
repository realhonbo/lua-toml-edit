local tomledit = require "toml_edit"

local source = [=[
# top comment
server_address = "edge.tomledit.com"
server_port    = 57000
author.method  = "token"
author.token   = "whatthefuck"
uid = "1145141919810"

[[proxies]]
name = "ssh"
type = "tcp"
ip   = "127.0.0.1"
port = 22
transport.useEncryption = true

[[proxies]]
name = "webui"
type = "http"
ip   = "127.0.0.1"
port = 80
subdomain = "laotie666"
]=]

local doc = tomledit.parse(source)

assert(doc:get("server_address") == "edge.tomledit.com")
assert(doc:get("server_port") == 57000)
assert(doc:get("uid") == "1145141919810")

assert(doc:contains("server_address") == true)
assert(doc:contains("server_port") == true)
assert(doc:contains("author.method") == true)
assert(doc:contains("author.token") == true)
assert(doc:contains("not_exists") == false)
assert(doc:contains("author.not_exists") == false)

assert(doc:get("author.method") == "token")
assert(doc:get("author.token") == "whatthefuck")

doc:set("server_port", 58000)
assert(doc:get("server_port") == 58000)

doc:set("author.token", "new_token_123")
assert(doc:get("author.token") == "new_token_123")

doc:set("uid", "abcdefg")
assert(doc:get("uid") == "abcdefg")

doc:set("enable_tls", true)
assert(doc:get("enable_tls") == true)

doc:set("retry_count", 3)
assert(doc:get("retry_count") == 3)

doc:set("server_name", "taishanpi")
assert(doc:get("server_name") == "taishanpi")

doc:set("tags", {"edge", "frp", "toml"})
local tags = doc:get("tags")
assert(type(tags) == "table")
assert(tags[1] == "edge")
assert(tags[2] == "frp")
assert(tags[3] == "toml")

doc:set("started_at", tomledit.raw("1979-05-27T07:32:00Z"))
assert(doc:get("started_at") ~= nil)

doc:set({"a.b", "c"}, "literal-dot-key")
assert(doc:get({"a.b", "c"}) == "literal-dot-key")

assert(doc:remove("server_name") == true)
assert(doc:get("server_name") == nil)
assert(doc:contains("server_name") == false)

assert(doc:remove("server_name") == false)
assert(doc:remove("not_exists") == false)

local out = doc:tostring()
assert(type(out) == "string")
assert(out:find("# top comment", 1, true))
assert(out:find('server_address = "edge.tomledit.com"', 1, true))
assert(out:find("server_port = 58000", 1, true))
assert(out:find('author.token = "new_token_123"', 1, true))
assert(out:find('uid = "abcdefg"', 1, true))
assert(out:find("enable_tls = true", 1, true))
assert(out:find("retry_count = 3", 1, true))
assert(out:find("started_at = 1979%-05%-27T07:32:00Z"))
assert(out:find("%[%[proxies%]%]"))
assert(out:find('name = "ssh"', 1, true))
assert(out:find('name = "webui"', 1, true))

-- twice parse
local reparsed = tomledit.parse(out)

assert(reparsed:get("server_address") == "edge.tomledit.com")
assert(reparsed:get("server_port") == 58000)
assert(reparsed:get("author.method") == "token")
assert(reparsed:get("author.token") == "new_token_123")
assert(reparsed:get("uid") == "abcdefg")
assert(reparsed:get("enable_tls") == true)
assert(reparsed:get("retry_count") == 3)
assert(reparsed:get({"a.b", "c"}) == "literal-dot-key")

local reparsed_tags = reparsed:get("tags")
assert(type(reparsed_tags) == "table")
assert(reparsed_tags[1] == "edge")
assert(reparsed_tags[2] == "frp")
assert(reparsed_tags[3] == "toml")

-- TODO:
local ok1, v1 = pcall(function()
    return doc:get("proxies.1.name")
end)
local ok2, v2 = pcall(function()
    return doc:get("proxies")
end)

assert(ok1 == true)
assert(ok2 == true)

-- table
assert(#v2 == 2)
assert(v2[1].name == "ssh")
assert(v2[1].type == "tcp")
assert(v2[1].ip == "127.0.0.1")
assert(v2[1].port == 22)

assert(v2[2].name == "webui")
assert(v2[2].type == "http")
assert(v2[2].ip == "127.0.0.1")
assert(v2[2].port == 80)
assert(v2[2].subdomain == "laotie666")

print("ok")
