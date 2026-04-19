TARGET_DIR ?= target/release
MODULE := toml_edit.so
LIB := $(TARGET_DIR)/libtoml_edit.so

.PHONY: all release clean

all: release

release: $(MODULE)

$(MODULE): Cargo.toml src/lib.rs
	cargo build --release
	cp $(LIB) $(MODULE)
	cp $(LIB) tests/$(MODULE)

clean:
	cargo clean
	rm -f $(MODULE)
