# Legacy FX pack bridge (transitional)

`cgfx-tools --project cgfx-project.json --pack` supports a layout entry whose source is a namespace-qualified declaration emitted by legacy `fxdata-build.py`:

```toml
[[entry]]
name = "eventTable"
namespace = "EventData"
source = { legacy = "EventData::eventTable" }

[[entry]]
name = "teamList"
namespace = "Teams"
source = { legacy = "Teams::teamList" }
```

When at least one `legacy` source occurs, the pack command copies the project input context (excluding only regenerable build artifacts) into a disposable `<project-parent>/.cgfx-legacy-bridge/legacy-fxdata-build` tree. It runs `<project>/Arduboy-Python-Utilities/fxdata-build.py` with `python3` against the copied `fxdata/fxdata.txt`, so legacy relative assets such as `../images/ArduFontTrimmed_5x6.png` resolve from the copy. It then reads only copied `fxdata/fxdata.h` and `fxdata/fxdata-data.bin`. Each requested declaration spans from its header address to the next declaration address, or `FX_DATA_BYTES` for the final declaration. Namespace-qualified names avoid ambiguity in nested legacy headers.

The bridge deletes its scratch copy after success or failure. It never writes the real `fxdata` tree, `src/fxdata.h`, or existing generated oracle outputs. Missing Python, script, input, generated header, generated data binary, symbol, or mismatched `FX_DATA_BYTES` is a hard error.

This is an option-3 migration bridge for symbolic `uint24_t` references (notably `event.txt` and `teams.txt`). Remove `source = { legacy = ... }`, `fxpack::legacy`, and `pack_with_legacy` after native pack builders can emit those declarations and resolve their symbolic addresses directly; retain ordinary `pack` behavior unchanged.
