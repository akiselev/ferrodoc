# Routing and research

Ferrodoc treats routing as a policy experiment over real observations, not as a synthetic classification exercise. The checked Phase 7 dataset binds each example to an actual `ferrodoc explain` trace and to complete benchmark case records. `router inspect` re-hashes those files and confirms that copied quality, failure, and measured cold-wall values still match the source report before any training or evaluation.

## Data lineage and leakage prevention

`ferrodoc-routing-dataset/1` uses feature schema `ferrodoc-routing-features/1`. Features are available before engine execution: page count, native non-whitespace character count, optional image coverage, and optional scanned likelihood. Missing values carry a reason and are never silently converted to zero. Benchmark truth, quality, latency, engine outcomes, and post-execution facts are not features.

Every record contains the input digest, a document-family identity, a corpus partition, a conversion-trace path/digest, and benchmark-report paths/digests. Validation rejects duplicate cases, unsafe paths, invalid features, copied outcomes that differ from their report, and a document family appearing in more than one partition. Related synthetic degradations must share the family identity, so they cannot cross training and evaluation boundaries.

The checked dataset uses the two purpose-built real regression documents as a small plumbing calibration: the born-digital document is the training case and the image-only document is the held-out routing case. It is not evidence of broad generalization.

## Baselines and learned admission

The router evaluates four deterministic baselines on an identical case set:

- always use valid native evidence;
- require a minimum native-character count before avoiding OCR;
- apply page-type rules with explicit missing-value behavior;
- apply deterministic profile-specific rules.

The only learned implementation is an auditable decision stump over native-character count. Its training objective declares quality, failure, and latency weights before fitting. Separate quality, latency, and failure results remain visible even though a scalar objective is used for model admission.

A model is qualified only if its feature coverage reaches the declared confidence floor and it beats every deterministic baseline on the identical held-out case set by the declared margin. The checked calibration is a retained negative result: the stump routes the image-only holdout to the failing native candidate, while the threshold/page/profile baselines select OCR. The model is serialized with `qualification.status = rejected` and is never activated.

At inference, `guarded_decision` receives the planner-approved engine set. An unqualified, incompatible, low-confidence, missing-feature, or hard-policy-rejected recommendation falls back in deterministic caller-provided order. The learned layer cannot add a candidate that the planner rejected for memory, VRAM, device, network, privacy, model availability, cost, or deadline policy.

## Experiment ledger

`ferrodoc-experiment-spec/1` binds the experiment ID, code/model/corpus/evaluator digests, comparison policy, exact command argument vectors, selected environment facts, protected truth/evaluator artifacts, raw report paths/digests, and cumulative evaluation/wall-time budgets. Mutation and evaluation trials are different tagged records.

`research run` deliberately executes no recorded command. Training or other mutation happens outside the evaluation runner and is recorded by output digest. Evaluation consists only of reading already-produced `BenchmarkReport` files, validating their complete case accounting, and comparing them with the trusted in-process evaluator. Protected truth and metric-code files are hashed before and after the run; output paths may not alias them. An existing ledger resumes only when every immutable experiment identity still matches.

Ledger writes are atomic. Failures remain visible, pending trials remain resumable within the cumulative budget, and the frontier retains tradeoffs, equal results, and comparisons made indeterminate by unknown evidence. It does not collapse the experiment to a throughput winner or substitute zero for an unknown resource.

## Reproduction

The offline checked path is:

```bash
./scripts/routing-smoke.sh
```

The underlying user commands are:

```bash
cargo run --locked -p ferrodoc -- router inspect . benchmarks/routing/dataset.json
cargo run --locked -p ferrodoc -- router train . benchmarks/routing/dataset.json router-model.json
cargo run --locked -p ferrodoc -- router evaluate . benchmarks/routing/dataset.json router-model.json
cargo run --locked -p ferrodoc -- router compare . benchmarks/routing/dataset.json router-model.json
cargo run --locked -p ferrodoc -- research run . experiment-spec.json experiment-ledger.json
cargo run --locked -p ferrodoc -- research status experiment-ledger.json
```

The checked raw reports retain measured cold wall time and explicit unknown RAM/VRAM. Re-generating a report creates a new observation and therefore a new digest; it is a new experiment input, not an in-place rewrite of historical evidence.
