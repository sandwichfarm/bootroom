# Vendored Web Dependencies — Version Pins

This directory holds bit-for-bit copies of upstream JavaScript and CSS that the
bootroom HTML loads as classic `<script>` / `<link>` tags. The bootroom binary
has **no npm toolchain** (per `CLAUDE.md`) and **must not load anything from a
CDN at runtime** (per `01-CONTEXT.md` `<specifics>`). Re-vendoring is a deliberate,
documented operation; see "Re-vendor procedure" below.

Fetched: 2026-05-17.

## File pins

| File          | Package    | Version | Source URL                                                         | Tarball path           | SHA-256 prefix     |
| ------------- | ---------- | ------- | ------------------------------------------------------------------ | ---------------------- | ------------------ |
| xterm.js      | xterm      | 5.3.0   | https://registry.npmjs.org/xterm/-/xterm-5.3.0.tgz                 | `package/lib/xterm.js` | `f0aea0f75f485590` |
| xterm.css     | xterm      | 5.3.0   | https://registry.npmjs.org/xterm/-/xterm-5.3.0.tgz                 | `package/css/xterm.css`| `832f3f2c603b43ad` |
| xterm-pty.js  | xterm-pty  | 0.12.0  | https://registry.npmjs.org/xterm-pty/-/xterm-pty-0.12.0.tgz        | `package/index.js`     | `2e7cbffea02dad1f` |

Full SHA-256 digests:

```
f0aea0f75f48559013ae6643c2479dd737d26da42d5524e6d2b70915ae6523c7  xterm.js
832f3f2c603b43ad4351ff04970150cc7a873014276db126a6065c6dd81e4872  xterm.css
2e7cbffea02dad1f72637c564534d104a13f9eec306deb9cc34fffe1faa58947  xterm-pty.js
```

File sizes (bytes): xterm.js 283404, xterm.css 5383, xterm-pty.js 12763 — total 301550.

## Version pin rationale

`xterm` 5.3.0 is the **unscoped** npm package. Do **not** bump to
`@xterm/xterm@6.x` — that is a different npm package with a breaking addon
API (renamed exports, ESM-only build, different DOM events). `xterm-pty` 0.12.0
was published against the 5.3.0 addon contract and uses `Terminal.loadAddon`
with the pre-6.x lifecycle; loading it on top of 6.x silently breaks the
master/slave wiring.

The two scripts MUST load in this order on the page: `xterm.js` first (defines
the global `Terminal` constructor), then `xterm-pty.js` (calls `loadAddon` on
a `Terminal` instance, so the `Terminal` global must already exist).

`xterm-pty` 0.12.0 lists `@xterm/xterm` as a peer-style runtime dep in its
`package.json`, but the actual UMD bundle (`index.js`) only reads `Terminal`
from the global scope at runtime. Loading our `xterm@5.3.0` UMD as a classic
script defines `window.Terminal` and satisfies that runtime lookup; the npm
peer dep is purely a packaging hint we ignore by design.

## Re-vendor procedure

Run the following from the repository root. `VERSION_XTERM` and `VERSION_PTY`
are the only knobs.

```bash
VERSION_XTERM=5.3.0
VERSION_PTY=0.12.0

# Clean workspace
rm -rf /tmp/xterm-pkg /tmp/xterm-pty-pkg /tmp/xterm.tgz /tmp/xterm-pty.tgz

# Download
curl -fsSL -o /tmp/xterm.tgz \
  "https://registry.npmjs.org/xterm/-/xterm-${VERSION_XTERM}.tgz"
curl -fsSL -o /tmp/xterm-pty.tgz \
  "https://registry.npmjs.org/xterm-pty/-/xterm-pty-${VERSION_PTY}.tgz"

# Extract
mkdir -p /tmp/xterm-pkg /tmp/xterm-pty-pkg
tar -xzf /tmp/xterm.tgz -C /tmp/xterm-pkg
tar -xzf /tmp/xterm-pty.tgz -C /tmp/xterm-pty-pkg

# Copy into vendor/
cp /tmp/xterm-pkg/package/lib/xterm.js     crates/bootroom/web/vendor/xterm.js
cp /tmp/xterm-pkg/package/css/xterm.css    crates/bootroom/web/vendor/xterm.css
cp /tmp/xterm-pty-pkg/package/index.js     crates/bootroom/web/vendor/xterm-pty.js

# Verify the LICENSE files in the tarballs haven't drifted, then re-copy
# their contents into LICENSES.md if they have.
diff <(cat crates/bootroom/web/vendor/LICENSES.md) - <<'EOF'
# Compare against /tmp/xterm-pkg/package/LICENSE and /tmp/xterm-pty-pkg/package/LICENSE.txt
EOF

# Re-compute SHA-256 and update this file
sha256sum crates/bootroom/web/vendor/xterm.js \
          crates/bootroom/web/vendor/xterm.css \
          crates/bootroom/web/vendor/xterm-pty.js
```

After running, update the table above and the digest block to match the new
hashes. Commit message convention: `chore(vendor): bump xterm to <version>`.

## Globals exposed

All three files are loaded by plan 01-06's HTML as **classic `<script>` tags
(NOT ES modules)** so the side-effectful UMD wrappers can attach globals.

| File          | Global attached    | Used by                                                |
| ------------- | ------------------ | ------------------------------------------------------ |
| xterm.js      | `window.Terminal`  | `new Terminal()` in the bootroom UI mount code         |
| xterm-pty.js  | `window.openpty`   | `const { master, slave } = openpty()` for QEMU chardev |
| xterm.css     | (stylesheet)       | terminal cell rendering; loaded via `<link rel="stylesheet">` |

The `xterm-pty` UMD bundle starts with the canonical pattern
`(function(g,f){...; for(var i in m) g[i]=m[i]}(globalThis, function(){...}))`,
which iterates the inner module's exports and copies each one onto the host
global object. The export named `openpty` therefore becomes `window.openpty`
when run in a browser without an AMD/CommonJS loader present.

## Licensing

Upstream MIT license texts for both packages are reproduced verbatim in
[LICENSES.md](./LICENSES.md). The bootroom binary embeds these files via
`include_dir!`; they ship inside the final release artifact and so must travel
with the attribution.
