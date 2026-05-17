# bootroom Makefile
#
# Only target right now is `qemu-assets`, which (re)builds the embedded
# qemu-wasm artifacts from the qemu-wasm submodule via docker. Run this
# whenever the qemu-wasm submodule pinned commit changes. Output is
# committed to git so end users do NOT need docker.
#
# See crates/bootroom/assets/qemu/REBUILD.md for the full procedure.

.PHONY: qemu-assets clean-qemu-assets help

QEMU_WASM_DIR := qemu-wasm
QEMU_OUT_DIR  := crates/bootroom/assets/qemu
QEMU_BUILDER  := build-qemu-wasm
EMSDK_VERSION := 3.1.50
PACK_DIR      := /tmp/bootroom-pack
HTDOCS_DIR    := /tmp/bootroom-htdocs

help:
	@echo "bootroom Makefile targets:"
	@echo "  qemu-assets       Rebuild qemu-wasm artifacts (requires docker; 10-30 minutes)"
	@echo "  clean-qemu-assets Remove generated qemu artifacts from $(QEMU_OUT_DIR)"

qemu-assets:
	@command -v docker >/dev/null || { echo "ERROR: docker is required to (re)build qemu-wasm artifacts" >&2; exit 1; }
	@test -d $(QEMU_WASM_DIR) || { echo "ERROR: $(QEMU_WASM_DIR) submodule missing; run 'git submodule update --init --recursive'" >&2; exit 1; }
	@test -f $(QEMU_WASM_DIR)/Dockerfile || { echo "ERROR: $(QEMU_WASM_DIR)/Dockerfile missing" >&2; exit 1; }
	@echo ">>> Step 1/5: Building qemu-wasm builder image (one-time; ~10-20 minutes on first run)..."
	cd $(QEMU_WASM_DIR) && docker build -t buildqemu - < Dockerfile
	@echo ">>> Step 2/5: Starting builder container..."
	-docker rm -f $(QEMU_BUILDER) >/dev/null 2>&1
	docker run --rm -d --name $(QEMU_BUILDER) -v $(PWD)/$(QEMU_WASM_DIR):/qemu/:ro buildqemu
	@echo ">>> Step 3/5: Compiling qemu-system-riscv64 inside the builder..."
	docker exec $(QEMU_BUILDER) /bin/sh -c '\
	  EXTRA_CFLAGS="-O3 -g -Wno-error=unused-command-line-argument -matomics -mbulk-memory -DNDEBUG -DG_DISABLE_ASSERT -D_GNU_SOURCE -sASYNCIFY=1 -pthread -sPROXY_TO_PTHREAD=1 -sFORCE_FILESYSTEM -sALLOW_TABLE_GROWTH -sTOTAL_MEMORY=2300MB -sWASM_BIGINT -sMALLOC=mimalloc --js-library=/build/node_modules/xterm-pty/emscripten-pty.js -sEXPORT_ES6=1 -sASYNCIFY_IMPORTS=ffi_call_js" && \
	  emconfigure /qemu/configure --static --target-list=riscv64-softmmu --cpu=wasm32 --cross-prefix= \
	    --without-default-features --enable-system --with-coroutine=fiber --enable-virtfs \
	    --extra-cflags="$$EXTRA_CFLAGS" --extra-cxxflags="$$EXTRA_CFLAGS" \
	    --extra-ldflags="-sEXPORTED_RUNTIME_METHODS=getTempRet0,setTempRet0,addFunction,removeFunction,TTY,FS" && \
	  emmake make -j$$(nproc) qemu-system-riscv64'
	@echo ">>> Step 4/5: Building the preload pack (kernel + opensbi) and packaging it..."
	rm -rf $(PACK_DIR) && mkdir -p $(PACK_DIR)
	docker build --output=type=local,dest=$(PACK_DIR) $(QEMU_WASM_DIR)/examples/riscv64/image
	cp $(QEMU_WASM_DIR)/pc-bios/opensbi-riscv64-generic-fw_dynamic.bin $(PACK_DIR)/
	docker cp $(PACK_DIR) $(QEMU_BUILDER):/pack
	docker exec $(QEMU_BUILDER) /bin/sh -c "cd /build && /emsdk/upstream/emscripten/tools/file_packager.py qemu-system-riscv64.data --preload /pack > load.js"
	@echo ">>> Step 5/5: Copying artifacts to $(QEMU_OUT_DIR)..."
	@mkdir -p $(QEMU_OUT_DIR)
	docker cp $(QEMU_BUILDER):/build/qemu-system-riscv64 $(QEMU_OUT_DIR)/out.js
	docker cp $(QEMU_BUILDER):/build/qemu-system-riscv64.wasm $(QEMU_OUT_DIR)/qemu-system-riscv64.wasm
	docker cp $(QEMU_BUILDER):/build/qemu-system-riscv64.worker.js $(QEMU_OUT_DIR)/qemu-system-riscv64.worker.js
	docker cp $(QEMU_BUILDER):/build/qemu-system-riscv64.data $(QEMU_OUT_DIR)/qemu-system-riscv64.data
	docker cp $(QEMU_BUILDER):/build/load.js $(QEMU_OUT_DIR)/load.js
	-docker rm -f $(QEMU_BUILDER) >/dev/null 2>&1
	@echo ">>> Done. Artifacts copied to $(QEMU_OUT_DIR)."
	@echo ">>> NOTE: module.js is bootroom-authored (NOT overwritten). Edit it manually if you change QEMU argv."
	@echo ">>> Remember to 'git add $(QEMU_OUT_DIR)' and commit."

clean-qemu-assets:
	rm -f $(QEMU_OUT_DIR)/out.js \
	      $(QEMU_OUT_DIR)/qemu-system-riscv64.wasm \
	      $(QEMU_OUT_DIR)/qemu-system-riscv64.worker.js \
	      $(QEMU_OUT_DIR)/qemu-system-riscv64.data \
	      $(QEMU_OUT_DIR)/load.js
	@echo "Cleaned generated qemu artifacts. module.js and REBUILD.md preserved."
