# Benchmarks

`benchmarks/foundry/` is the conventional location for a generated held-out foundry corpus. Generate it locally before running `experiments/foundry-routing.toml`:

```bash
ferrodoc foundry generate benchmarks/foundry --count 256 --seed 407912268
```

Do not commit huge generated corpora by default. Commit manifests/seeds and externally version larger benchmark datasets.
