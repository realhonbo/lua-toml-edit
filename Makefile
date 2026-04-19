TARGET_DIR ?= target/release
MODULE := toml_edit.so
LUA_VERSION ?= lua51
LUA ?= lua
CARGO_BUILD_FLAGS := --no-default-features --features $(LUA_VERSION)
UNAME_S := $(shell uname -s)

ifeq ($(UNAME_S),Darwin)
LIB_EXT := dylib
else
LIB_EXT := so
endif

LIB := $(TARGET_DIR)/libtoml_edit.$(LIB_EXT)

.PHONY: all release test clean

all: release

release:
	cargo build --release $(CARGO_BUILD_FLAGS)
	cp $(LIB) $(MODULE)
	cp $(LIB) tests/$(MODULE)

test: release
	$(LUA) tests/check.lua

clean:
	cargo clean
	rm -f $(MODULE)
	rm -f tests/$(MODULE)
