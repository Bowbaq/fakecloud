+++
title = "Conformance"
description = "How fakecloud verifies behavioral parity with real AWS."
weight = 2
+++

fakecloud aims for 100% behavioral parity with AWS on every operation it implements. That's a big claim — here's how we verify it.

## Two layers of tests

**Conformance** checks AWS request/response shape and validation behavior. It's generated from AWS's own Smithy models.

**E2E** checks real behavior across services using the official AWS SDKs. Tests boot a real fakecloud server and make real SDK calls.

## How conformance works

We commit AWS's own Smithy models to [`aws-models/`](https://github.com/faiscadev/fakecloud/tree/main/aws-models) and generate test inputs with six orthogonal strategies:

1. **Boundary values** from `@length` and `@range` constraints
2. **Exhaustive enum coverage** — every enum value, every operation that uses it
3. **Optional-field permutations** — every combination of optional fields present or absent
4. **Property-based random generation** — seeded random inputs with shrinking
5. **Examples from the model's `@examples` trait** — the canonical inputs AWS documents
6. **Negative tests** for each constraint — values that should fail validation

Every response is validated against the operation's Smithy output shape. Missing required fields, unexpected fields, and wrong types are all failures. The baseline ([`conformance-baseline.json`](https://github.com/faiscadev/fakecloud/blob/main/conformance-baseline.json)) is checked in CI — any regression blocks the merge.

## Current coverage

54,000+ generated test variants, covering every operation in every implemented service. 22 services at 100% conformance.

See the harness and methodology at [`crates/fakecloud-conformance/`](https://github.com/faiscadev/fakecloud/tree/main/crates/fakecloud-conformance).

## Reproducing locally

```sh
cargo run -p fakecloud-conformance -- run --services s3
```

Omit `--services` to run the full suite. Expect a few minutes; some services have thousands of variants.

## Why schema-driven

The critical property is that the conformance tests are generated from AWS's own API definitions, not written by hand. That means:

- They cover every operation AWS has documented, not just the ones we remembered to test.
- They can't drift from AWS — when AWS updates a model, we pull the new model and the tests regenerate.
- They can't be gamed by code that passes them accidentally. The tests check response structure against the authoritative schema, not against what fakecloud happens to return.

This is the difference between "our tests pass" and "we match AWS." The second is what actually matters for an emulator.
