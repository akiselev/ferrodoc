# Model manifests

Ferrodoc model manifests contain immutable file sizes and SHA-256 digests, source and revision identity, license metadata, and any required acceptance prompt. Model binaries are not part of the repository.

Install an already acquired OCRS model pair without network access:

```bash
cargo run --locked -p ferrodoc -- models pull \
  --manifest models/ocrs-cpu.json \
  --source /path/to/ocrs-models \
  --store /path/to/ferrodoc-model-store \
  --accept
```

`models pull` validates every source file before atomically publishing a logical view. It does not download a manifest or model. The source directory must contain `text-detection.rten` and `text-recognition.rten` with the exact checked-in digests.
