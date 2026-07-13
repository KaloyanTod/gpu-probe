# Security review: `gpu-probe`

**Review date:** 2026-07-10  
**Reviewed revision:** `18c9586`  
**Scope:** Rust sources, WGSL shader, build metadata, CLI, SQLite writer, and the integrity of generated benchmark results.

## Executive summary

`gpu-probe` is a reasonable **honest-host local benchmark**, but it is **not safe to use as a financial or consensus oracle when the benchmark operator controls the machine**. The current result is entirely self-attested: the same host supplies the executable, GPU driver, adapter identity, GPU timestamps, CPU clock, verification result, validity flags, and stored row. There is no independent challenge, result signature, trusted clock, measured binary, hardware attestation, or server-side verification.

The answer to the main question is therefore **yes: a participant who controls or compromises the benchmark host can change the reported result**. A patched client can directly construct plausible result values. A malicious or instrumented GPU driver can falsify the adapter identity, timestamp values, and output buffer while the application remains unchanged. A distributed database will not fix this trust problem; it can make false submissions durable and replicated.

There is also a concrete correctness bypass in the current verifier: an output containing `NaN` at every sampled cell is recorded as `verification_passed = 1` with `max_rel_error = 0`. A temporary regression test created during this review confirmed the bypass. The test file was removed after execution.

**Release recommendation:** do not make payments, allocation decisions, consensus decisions, or investment-affecting decisions from these results until the critical and high-severity issues below are addressed at the protocol level. Some of the problem cannot be solved by ordinary client-side Rust code alone.

## Threat model and important distinction

This review considers:

- a participant who wants to receive a better score or reward;
- malware or another process with control of the benchmark process;
- a modified build of this open-source module;
- a custom, hooked, or compromised Vulkan/DX12/Metal/OpenGL driver;
- a virtual GPU or software adapter that reports a misleading device type;
- unstable, overclocked, undervolted, or faulty GPU hardware;
- future submission of results to a distributed database.

No network listener or untrusted network parser exists in this repository. An outside attacker with **no code execution, no driver control, no database access, and no control of benchmark parameters** has no obvious remote entry point in this module alone. The serious threat is a dishonest claimant or a compromised claimant machine, which is normally the correct threat model for reward-bearing crypto participation.

## Findings overview

| ID | Severity | Finding | Direct result-integrity impact |
|---|---:|---|---|
| GP-001 | Critical | No trustworthy root of execution or result provenance | A controlled host can submit arbitrary results |
| GP-002 | Critical | `NaN` values bypass output verification | Invalid GPU output can be marked as verified with zero error |
| GP-003 | High | Verification samples are fixed, sparse, and check only the final dispatch | Selective computation and corruption can evade checking |
| GP-004 | High | GPU identity and timing are unauthenticated driver claims | A fake driver/ICD can impersonate hardware and forge speed |
| GP-005 | High | No authoritative challenge, canonical policy, or replay protection | Participants can choose parameters, replay, and cherry-pick |
| GP-006 | High | `run_valid` does not include verification success | Downstream code can accept a failed computation as valid |
| GP-007 | Medium | Hashes are non-cryptographic, incomplete, and self-reported | Shader/input substitution is not securely detectable |
| GP-008 | Medium | Non-finite timing values and hostile sizes are not safely rejected | Invalid state may pass or panic/terminate the process |
| GP-009 | Medium | Fastest-sample scoring and a single broad ceiling are gameable | One manipulated timestamp can inflate the headline result |
| GP-010 | Informational | One transitive dependency is unmaintained | Supply-chain maintenance risk, not a known active vulnerability |

## Detailed findings

### GP-001 — No trustworthy root of execution or result provenance

**Severity: Critical**

All evidence is generated inside the environment whose performance is being claimed. [`run_probe`](src/lib.rs#L104) creates ordinary public Rust values, and the fields of [`BenchmarkResult`](src/benchmark.rs#L113), [`HardwareInfo`](src/hardware.rs#L10), [`VerifyResult`](src/verify.rs#L16), and [`ProbeOutcome`](src/lib.rs#L72) are public. [`store::insert`](src/store.rs#L60) is also public and accepts those caller-supplied structures.

A modified embedding application does not need to run the GPU workload. It can construct a `BenchmarkResult` containing an attacker-selected time below the 200,000 GFLOP/s ceiling, a passing `VerifyResult`, and desired hardware strings, then store or submit it. Alternatively, it can run the honest probe and alter fields after verification. Nothing in the result proves that:

- the published source was used;
- the expected executable and dependencies were loaded;
- the recorded shader was the code executed by the driver;
- the claimed GPU executed the commands;
- the timing values came from genuine hardware execution;
- the record is fresh and belongs to a particular challenge;
- the record was not changed between measurement and submission.

Signing the result with a normal software-held key would authenticate the participant, but would **not** prove that the participant ran honest code. A modified client can ask the same key to sign fabricated data.

**Impact:** complete score, identity, metadata, verification, and validity forgery by a participant controlling the worker.

**Recommendation:** treat every client result as untrusted input. Before financial use, define an authoritative verifier protocol with fresh challenges, canonical acceptance rules, replay prevention, a canonical signed result envelope, and a trustworthy execution mechanism. Where genuine remote attestation of the executable, OS/driver, clock, and GPU is unavailable, do not claim that consumer-controlled machines can produce cryptographically trustworthy performance measurements. Use trusted workers, an auditable controlled test environment, or a protocol whose correctness is independently verifiable and whose security does not depend on self-reported elapsed time.

### GP-002 — `NaN` values bypass output verification

**Severity: Critical**

The verifier calculates relative error and updates the maximum only when `rel > max_rel_error` ([`verify.rs:50-56`](src/verify.rs#L50)). IEEE-754 comparisons with `NaN` are false. If `gpu` is `NaN`, `rel` is `NaN`, the update is skipped, and `max_rel_error` stays at its initial value of zero. The final `0.0 <= 1e-3` comparison passes ([`verify.rs:59-62`](src/verify.rs#L59)).

During this review, the following behavior was confirmed in a temporary test using `n = 16`, honest generated inputs, and a `C` buffer filled entirely with `f32::NAN`:

```text
result.passed == true
result.max_rel_error == 0.0
```

This can be triggered by a malicious driver or modified GPU output. Some hardware instability also produces non-finite values, so this is not limited to source-code modification.

**Impact:** completely invalid output can be labeled verified.

**Recommendation:** fail closed on every non-finite input, reference, GPU value, absolute error, and relative error. Initialize failure conservatively and explicitly reject `!value.is_finite()`. Validate buffer lengths and `n` before indexing. Add regression tests for `NaN`, positive/negative infinity, truncated buffers, zero size, and extreme finite values.

### GP-003 — Verification samples are fixed, sparse, and check only the final dispatch

**Severity: High**

The verifier always uses the same public seed and 32 coordinates ([`verify.rs:13-14`](src/verify.rs#L13), [`verify.rs:35-42`](src/verify.rs#L35)). For the default `N = 1024`, those 32 cells cover only **0.003052%** of the 1,048,576-cell output. The coordinates can be calculated before execution. A malicious shader, driver, or firmware implementation can return correct dot products only for those known cells and arbitrary values elsewhere.

The readback occurs once, after all warm-up and timed dispatches ([`benchmark.rs:446-447`](src/benchmark.rs#L446)). Consequently, none of the earlier timed dispatch outputs are tied to verification. A hostile implementation can report very short timings for skipped or incorrect dispatches and provide a valid final sampled output.

For non-adversarial random corruption, 32 samples are also weak at low error rates. If 1% of cells are independently corrupt, the probability that 32 samples detect at least one is only about **27.5%**. Predictable sampling makes the adversarial case much worse than this probability suggests.

**Impact:** selective computation, intermittent faults, and earlier incorrect timed iterations can evade verification.

**Recommendation:** the verifier—not the claimant—should choose unpredictable verification randomness **after** the claimant commits to the complete output. Commit to the full `C` buffer with a cryptographic hash or Merkle root, then reveal random coordinates and request proofs. Increase the sample count according to a documented soundness target. For stronger verification, consider a protocol such as Freivalds-style randomized matrix-product checking; a finite-field workload is much easier to verify soundly than approximate `f32` arithmetic. Bind verification to timed iterations rather than checking only an unlinked final output.

Random sampling improves computation correctness but does not independently prove elapsed time. Timing still needs a trusted measurement source or a different protocol design.

### GP-004 — GPU identity and timing are unauthenticated driver claims

**Severity: High**

Adapter name, vendor/device IDs, device type, driver strings, backend, feature support, timestamp ticks, and timestamp period all cross the local GPU API/driver boundary. The application trusts them without attestation ([`benchmark.rs:161-177`](src/benchmark.rs#L161), [`hardware.rs:73-85`](src/hardware.rs#L73), [`benchmark.rs:362-423`](src/benchmark.rs#L362)).

Only `DeviceType::Cpu` is rejected ([`benchmark.rs:163-169`](src/benchmark.rs#L163)). `VirtualGpu` and `Other` are accepted, and a custom or compromised driver can simply claim `DiscreteGpu`. `force_fallback_adapter: false` is a selection request, not authentication. Raw adapter JSON is not independent corroboration because it comes from the same source.

On the timestamp path, the score is derived from two driver/GPU-provided query values and a driver-provided timestamp period. A malicious implementation can return a plausible positive duration that stays below the broad sanity ceiling. On the wall-clock path, a hooked process can manipulate submission/wait behavior or the result after measurement.

The smoke test performed during this review enumerated devices classified as `DiscreteGpu`, `IntegratedGpu`, `Cpu`, and `Other`, demonstrating that non-CPU classifications are normal inputs to the selection path. This is not itself an exploit, but it confirms that the CPU-only check is not an identity boundary.

**Impact:** hardware impersonation and arbitrary score inflation without patching the Rust source, when the driver/API stack is controlled.

**Recommendation:** treat adapter metadata as descriptive only. If device identity affects rewards, require a vendor-backed device certificate/attestation chain and bind it to a fresh challenge and measured workload. Verify attestation at an independent authority. If the target GPU ecosystem cannot attest compute execution and trustworthy timestamps, document that model identity and speed cannot be proven from an adversarial host.

### GP-005 — No authoritative challenge, canonical policy, or replay protection

**Severity: High**

The claimant controls `n`, seed, warm-up count, timed count, and database path ([`main.rs:26-66`](src/main.rs#L26)). The default seed is permanent and public ([`lib.rs:56-64`](src/lib.rs#L56)). There is no challenge ID, verifier nonce, expiry, submission identity, sequence number, or consumed/replayed state.

The README's comparability invariant lists shader hash, `wgpu` version, backend, and timing method, but omits at least matrix size, seed, iteration policy, precision, tile size, build identity, and device policy ([`README.md:99-102`](README.md#L99)). Merely recording parameters does not ensure that downstream code filters on all of them.

The code accepts `--warmup 0 --iters 1`; a smoke test confirmed this configuration can produce `run_valid = true`. A claimant can repeat runs and submit only a favorable result. The local timestamp comes from the claimant's system clock and is not trustworthy freshness evidence ([`util.rs:24-29`](src/util.rs#L24)).

**Impact:** replay, policy downgrade, selective submission, and accidental comparison of incompatible runs.

**Recommendation:** an independent authority should issue a unique, unpredictable, short-lived challenge containing the exact allowed configuration and protocol version. Derive inputs from the challenge, bind every field to its challenge ID, accept it once, and enforce all bounds and comparability rules at ingestion/consensus. Do not let a client-supplied `run_valid` or timestamp decide acceptance.

### GP-006 — `run_valid` does not include verification success

**Severity: High**

`BenchmarkResult::run_valid` covers timing discards, empty timings, and the GFLOP/s ceiling ([`benchmark.rs:476-494`](src/benchmark.rs#L476)). Verification is calculated later and does not update that flag ([`lib.rs:117-120`](src/lib.rs#L117)). Both values are stored separately ([`store.rs:111-113`](src/store.rs#L111)).

The CLI correctly combines both flags when selecting its exit code ([`main.rs:170-171`](src/main.rs#L170)), but an embedding crypto project or database query may reasonably interpret `run_valid = true` as overall validity and fail to filter `verification_passed`. Invalid and failed rows are intentionally stored.

**Impact:** a failed computation can be accepted as a valid benchmark through a likely integration mistake.

**Recommendation:** expose a single fail-closed acceptance state or method that includes verification, timing validity, canonical policy, finite-value checks, attestation/challenge validation, and any required signatures. Keep diagnostic sub-flags, but never ask downstream financial logic to reconstruct security policy from several booleans. The authoritative verifier must calculate this state itself.

### GP-007 — Hashes are non-cryptographic, incomplete, and self-reported

**Severity: Medium**

The shader identifier is unkeyed 64-bit FNV-1a ([`benchmark.rs:62-70`](src/benchmark.rs#L62)). FNV is useful for accidental-change detection but is not collision resistant. The input fingerprint hashes only the first 256 bytes (64 `f32` values) of `A`; it does not cover the rest of `A` or any of `B` ([`benchmark.rs:88-95`](src/benchmark.rs#L88)). The README's statement that this proves identical inputs is therefore too strong.

The shader hash covers WGSL source requested by the honest application, not compiled backend code or proof of what a hostile driver executed. The recorded `wgpu_version` is parsed from the local manifest at build time ([`build.rs:20-32`](build.rs#L20)); it does not bind the executable, compiler, build flags, complete dependency graph, Cargo source patches, or driver.

**Impact:** collisions, substitutions outside the fingerprinted prefix, and custom builds can share expected identifiers. On a controlled host, even a cryptographic hash remains a claim unless independently measured or attested.

**Recommendation:** use a cryptographic digest such as SHA-256 or BLAKE3 over a canonical transcript that includes the full challenge, complete `A`, complete `B`, committed `C`, shader bytes, executable/build identity, dependency lock, toolchain, configuration, and raw measurements. Verify or regenerate those values independently. Use reproducible, signed releases and measured-boot/attestation evidence where available.

### GP-008 — Non-finite timing values and hostile sizes are not safely rejected

**Severity: Medium**

Timing checks use `elapsed_ms <= 0.0`, which does not reject `NaN` ([`benchmark.rs:413-419`](src/benchmark.rs#L413), [`benchmark.rs:429-440`](src/benchmark.rs#L429)). A `NaN` later reaches `partial_cmp(...).unwrap()` and can panic ([`benchmark.rs:449-452`](src/benchmark.rs#L449)). Positive infinity can survive as a timing and leave `run_valid = true`, although it produces a zero score. Timestamp period is not explicitly checked for finite, positive, or plausible values.

`normalize_n` rounds with unchecked `u32` arithmetic ([`util.rs:11-19`](src/util.rs#L11)). Values near `u32::MAX` can panic in debug builds or wrap in release builds. Matrix element counts and allocations have no configured upper bound. Very large iteration/warm-up counts can consume excessive time, while malformed public-library calls to verification can panic on zero `n` or short buffers.

**Impact:** denial of service, panics, invalid persisted values, and fail-open behavior in future integrations that accept remotely supplied configuration.

**Recommendation:** validate all external configuration before GPU allocation: canonical `n`, checked multiplication/addition, maximum memory/work estimate, exact or bounded iteration counts, and nonzero buffer dimensions. Return typed errors instead of panicking. Require every raw timing, period, aggregate, error, and score to be finite, positive where applicable, internally consistent, and within protocol bounds.

### GP-009 — Fastest-sample scoring and a single broad ceiling are gameable

**Severity: Medium**

The benchmark exposes and prominently prints the minimum duration/highest score ([`benchmark.rs:449-474`](src/benchmark.rs#L449), [`main.rs:117-124`](src/main.rs#L117)). The only anti-inflation check is a universal ceiling of 200,000 GFLOP/s, and it invalidates only values strictly above the ceiling ([`benchmark.rs:25-28`](src/benchmark.rs#L25), [`benchmark.rs:489-493`](src/benchmark.rs#L489)). A malicious source can report any desired value below that limit. Even on an honest noisy system, the minimum rewards a single favorable outlier and repeated-run cherry-picking.

**Impact:** inflated headline scores and unstable reward calculations, especially if downstream code uses `min_gflops`.

**Recommendation:** do not use the minimum for financial decisions. Enforce an exact sampling policy, minimum iteration count, robust statistics, variance/quantile bounds, raw-transcript consistency, and independent plausibility rules. These are useful anomaly detectors but are not substitutes for trusted measurement.

### GP-010 — One transitive dependency is unmaintained

**Severity: Informational**

`cargo audit` 0.22.2 scanned 149 locked crates against 1,159 RustSec advisories. It found no known vulnerabilities and one allowed warning: `paste 1.0.15` is unmaintained (`RUSTSEC-2024-0436`). It is a transitive dependency in the GPU stack.

**Impact:** no known direct exploit from this warning, but an unmaintained crate may not receive future fixes.

**Recommendation:** run `cargo audit` and dependency-policy checks in CI, monitor the upstream `wgpu` dependency path that brings in `paste`, and update when upstream removes or replaces it. Pin and archive release inputs, while still maintaining a deliberate security-update process.

## Practical attack paths

### 1. Modified client or embedding application

The participant builds a compatible binary that constructs public result structures directly, reports a plausible duration below the ceiling, sets both booleans to true, copies expected strings/hashes, and submits the record. No GPU work is required. A distributed database cannot distinguish this from the honest process unless its validators independently verify stronger evidence.

### 2. Malicious driver or custom graphics implementation

The participant runs the published binary but controls the graphics API stack. The driver advertises a desired GPU identity and timestamp support, returns attacker-selected query ticks/period, and returns correct values only for the known verification cells—or returns `NaN`, exploiting GP-002. The application's locally calculated shader hash remains unchanged even though it does not prove what the driver executed.

### 3. Hardware instability or manipulation

Overclocking can improve real measured performance; whether this is allowed must be a protocol rule. Undervolting, faulty memory, thermal behavior, or transient computation errors can corrupt unsampled cells. Only the last output is sampled, and `NaN` currently passes. Hardware model strings do not prove stock clocks, power limits, cooling, GPU count, or absence of virtualization.

### 4. Replay and selective submission

The workload and defaults are predictable, timestamps are claimant-controlled, and there is no one-time challenge. A participant can run repeatedly, discard poor measurements, and replay or submit the most favorable valid-looking row.

## Recommended architecture before financial use

### Immediate code fixes (necessary, not sufficient)

1. Reject all non-finite values in verification and timing; add regression tests.
2. Make overall acceptance fail closed and include correctness, timing, policy, freshness, and provenance.
3. Add strict checked bounds for matrix size, allocation size, iteration counts, timestamps, aggregates, and buffer lengths.
4. Stop presenting fixed 32-cell checking as proof. Clearly label it an honest-host fault detector until a challenge protocol exists.
5. Replace FNV/partial fingerprints with cryptographic full-transcript digests.
6. Avoid using `min_gflops` for decisions; require robust statistics and consistency checks.

### Protocol requirements

1. Have an independent verifier issue a fresh one-time challenge with exact parameters and expiry.
2. Bind the challenge, participant identity, protocol version, binary/build identity, complete input/output commitments, raw timing transcript, and hardware evidence into one canonical signed envelope.
3. Commit to output before the verifier reveals unpredictable checking randomness. Verify enough random positions or use a sound randomized matrix-product proof.
4. Enforce replay protection and all acceptance rules at every validator/ingestion boundary. Never trust client-computed booleans.
5. Use vendor-backed device and execution attestation if the chosen GPU platform genuinely supports the required claims. Verify certificate chains and freshness independently.
6. If trustworthy GPU execution/time attestation is unavailable, use controlled benchmark operators, redundant independent observation with economic controls, or redesign the crypto mechanism so funds do not depend on self-reported benchmark time.

### Storage requirements

A distributed database is useful for availability and tamper-evident history, but it is not an oracle. Store signed canonical envelopes and verifier decisions, retain invalid submissions separately, and make validators reject malformed, replayed, policy-incompatible, unauthenticated, or unverifiable records before they influence balances.

## Positive observations

- SQL values are passed through `rusqlite` parameters rather than string interpolation ([`store.rs:72-128`](src/store.rs#L72)); no SQL injection was found in the insert path.
- GPU buffers and shader bindings use safe `wgpu` abstractions; no project-local `unsafe` Rust was found.
- The WGSL kernel's indexing is consistent with normalized tile-aligned sizes under honest configuration.
- The CLI exits unsuccessfully when either current timing validity or verification fails.
- Direct dependency versions are mostly pinned, the test suite passes, and the dependency audit found no known active vulnerability.
- Raw timings and extensive metadata are retained, which is useful for diagnostics even though they are not authenticated.

## Validation performed

- Manual source review of all Rust, WGSL, manifest, build, README, and example files.
- `cargo test --all-features`: 6 unit tests and 1 documentation test passed.
- Temporary targeted regression test: confirmed an all-`NaN` `C` buffer passes verification with zero recorded error; temporary file removed.
- Local smoke run with `N=16`, zero warm-ups, and one timed iteration: completed with verification pass and `run_valid = true`, confirming permissive client policy.
- Fixed-coordinate analysis: 32 unique checked cells for `N=1024`, or 0.003052% of output.
- `cargo audit` 0.22.2: 149 locked dependencies scanned; no known vulnerability, one unmaintained transitive crate warning.
- `cargo clippy --all-targets --all-features -- -D warnings`: stopped on two non-security style lints (`manual_is_multiple_of`); no security conclusion should be drawn from Clippy alone.

No custom malicious GPU driver, firmware modification, physical fault injection, fuzzing campaign, or formal verification was performed. Those require a separate lab effort, but their absence does not change the architectural self-attestation finding.

## Final assessment

The benchmark can detect ordinary implementation mistakes on a cooperative machine, but it cannot establish truthful performance or hardware identity against the party running it. Fixing the `NaN` bypass, unpredictable verification, strict validation, and result-state semantics will materially improve correctness. Protecting investments additionally requires moving trust out of the claimant-controlled process. Until that architecture exists, results from this module should be treated as untrusted telemetry and must not directly determine financial outcomes.
