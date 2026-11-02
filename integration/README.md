# Build integration

`overlays.json` maps owned source directories to their build locations and orders
the integration patches. `patches/base.patch` connects performance/composition to
pristine upstream. `patches/native-gpu.patch` connects native drawing, including
the changes it needs in retained scene and frame paths.

`workspace/` contains the canonical Cargo manifest and lockfile. `assemble.py`
verifies pristine upstream, stages the source layers, checks/applies patches and
synchronizes `target/integration/workspace/`. Existing generated edits are
protected. `script/cargo` serializes Cargo with assembly and persists successful
lockfile updates. No implementation code belongs in patches.

```sh
python3 script/assemble
python3 script/check-upstream
python3 -m unittest discover -s integration -p 'test_*.py'
```

See the [sync guide](../docs/upstream-sync.md) for updating upstream and the
limitations of a source overlay compared with an upstream-supported plugin API.
