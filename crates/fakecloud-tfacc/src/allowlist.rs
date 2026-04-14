//! Two-layer opt-in for upstream `terraform-provider-aws` acceptance tests.
//!
//! Layer 1: `SERVICES` is an allow-list. A service is only exercised at all
//! if it appears here. This matches fakecloud's parity-per-implemented-service
//! invariant — we don't want CI noise from services we don't claim to
//! support.
//!
//! Layer 2: each `Service` carries a `deny` array of specific upstream test
//! names to skip, with reasons grouped in inline comments. These are passed
//! to `go test -skip '^(name1|name2|...)$'` (Go ≥ 1.20).
//!
//! Deny-list semantics:
//!
//! * **unsupportable**: the test needs a fakecloud feature that we don't
//!   plan to implement (cross-region replicas, real backup encryption,
//!   import from S3). Stays denied permanently.
//! * **gap**: the test fails because of a real fakecloud bug. Denied
//!   temporarily — driving these to zero is the point of later batches.
//! * **hung**: the test never completed in our initial triage run. Denied
//!   until we can characterise it; may move to gap or unsupportable later.
//!
//! Every entry must have a reason comment. Adding an entry without one is
//! a review-blocking mistake.

pub struct Service {
    /// Directory name under `internal/service/` — e.g. `sqs`, `dynamodb`.
    pub name: &'static str,
    /// Go `-run` regex. Narrow this to carve out a subset of a service's
    /// upstream tests while the rest of that service's deny-list is being
    /// populated. Widening (or removing the override) is the mechanism for
    /// growing coverage in later batches.
    pub run_regex: &'static str,
    /// Upstream test function names to skip, one per line, grouped by
    /// reason in inline comments.
    pub deny: &'static [&'static str],
}

pub const SERVICES: &[Service] = &[
    Service {
        name: "bedrock",
        // Batch 10: `data.aws_bedrock_foundation_models` data source
        // smoke. fakecloud's Bedrock implementation already returns the
        // expected ListFoundationModels shape, so this passes out of
        // the box. Resource tests (model invocation, guardrails) need
        // the Bedrock runtime container path to be plumbed through TF
        // and are deferred to a later batch.
        run_regex: "^TestAccBedrockFoundationModelsDataSource_basic$",
        deny: &[],
    },
    Service {
        name: "apigatewayv2",
        // Batch 9: core `aws_apigatewayv2_api` (HTTP) smoke. The fix
        // here is making CreateApi return four metadata fields that
        // real AWS always populates (api_key_selection_expression,
        // route_selection_expression, disable_execute_api_endpoint,
        // ip_address_type) — Terraform's provider asserts on each of
        // them on every refresh.
        run_regex: "^TestAccAPIGatewayV2API_basicHTTP$",
        deny: &[],
    },
    Service {
        name: "kinesis",
        // Batch 8: core `aws_kinesis_stream` smoke. The fix here is
        // making `IncreaseStreamRetentionPeriod` accept same-value as a
        // no-op — real AWS does this despite what the API docs say,
        // and the upstream provider unconditionally calls it with the
        // default 24h on every create.
        run_regex: "^TestAccKinesisStream_basic$",
        deny: &[],
    },
    Service {
        name: "sns",
        // Batch 7: core `aws_sns_topic` smoke. Passes against fakecloud
        // out of the box.
        run_regex: "^TestAccSNSTopic_basic$",
        deny: &[],
    },
    Service {
        name: "events",
        // Batch 7: core EventBridge `aws_cloudwatch_event_bus` and
        // `aws_cloudwatch_event_rule` smokes. Both pass out of the box.
        // Note: the upstream service directory is `events`, not
        // `eventbridge` — Terraform uses the legacy CloudWatch Events
        // naming.
        run_regex: "^TestAccEvents(Bus|Rule)_basic$",
        deny: &[],
    },
    Service {
        name: "kms",
        // Batch 6: core `aws_kms_key` smoke. Passes against fakecloud
        // out of the box.
        run_regex: "^TestAccKMSKey_basic$",
        deny: &[],
    },
    Service {
        name: "logs",
        // Batch 6: core `aws_cloudwatch_log_group` smoke. The fix here
        // is making DescribeLogGroups always return `logGroupClass`
        // (defaulting to STANDARD), which Terraform's provider asserts
        // on every refresh.
        run_regex: "^TestAccLogsGroup_basic$",
        deny: &[],
    },
    Service {
        name: "iam",
        // Batch 5: core CRUD smoke for the four most-used IAM resource
        // types. Passes against fakecloud out of the box — no
        // fakecloud-side changes needed. Later batches widen to
        // attached-policy, group-membership, and instance-profile tests.
        run_regex: "^TestAccIAM(Role|User|Policy|Group)_basic$",
        deny: &[],
    },
    Service {
        name: "ssm",
        // Batch 4: core `aws_ssm_parameter` smoke. The fix here is making
        // `lookup_param` tolerate the `name:version` selector that real
        // AWS accepts on GetParameter / ListTagsForResource — without it
        // the upstream import-with-version step fails with
        // InvalidResourceId.
        run_regex: "^TestAccSSMParameter_basic$",
        deny: &[],
    },
    Service {
        name: "secretsmanager",
        // Batch 4: core `aws_secretsmanager_secret` smoke. Passes against
        // fakecloud out of the box — no fakecloud-side changes needed.
        run_regex: "^TestAccSecretsManagerSecret_basic$",
        deny: &[],
    },
    Service {
        name: "sqs",
        // SQS tests are curated via a positive regex rather than
        // `^TestAcc` + deny-list because CI runners (2-core Linux) are
        // dramatically slower than dev machines — running the full 66
        // TestAcc set exceeds the 90m CI timeout. Adding a new batch
        // widens this regex by one cluster at a time.
        //
        // Batch 2: JSON canonicalization fix — redrive + policy round trip.
        // Batch 3: encryption defaults + mode-switch reset.
        run_regex: concat!(
            "^TestAccSQS(",
            // core queue smoke + JSON-canonicalized attributes
            "Queue_(basic|redrivePolicy|redriveAllowPolicy|Policy_basic",
            // encryption attribute round-trip
            "|encryption|managedEncryption",
            "|defaultKMSDataKeyReusePeriodSeconds",
            "|ManagedEncryption_kmsDataKeyReusePeriodSeconds",
            "|noEncryptionKMSDataKeyReusePeriodSeconds)",
            // separate resources for policy and redrive subresources
            "|QueuePolicy_basic",
            "|QueueRedrivePolicy_basic",
            "|QueueRedriveAllowPolicy_basic)$",
        ),
        deny: &[
            // --- hung: runs clean locally but never completes in CI,
            //          blocking the whole service at the 90m timeout.
            //          Needs characterisation in a follow-up batch. ---
            "TestAccSQSQueueRedriveAllowPolicy_basic",
        ],
    },
    Service {
        name: "dynamodb",
        // Batch 1: only the `aws_dynamodb_table` resource tests. The
        // upstream dynamodb service directory also has ~90 tests covering
        // table_item, replica, export, kinesis_streaming_destination, and
        // global_table which surface deeper fakecloud gaps and will be
        // added in follow-up batches.
        run_regex: "^TestAccDynamoDBTable_",
        deny: &[
            // --- unsupportable: DynamoDB Global Tables / cross-region replicas ---
            "TestAccDynamoDBTable_Replica_single",
            "TestAccDynamoDBTable_Replica_singleCMK",
            "TestAccDynamoDBTable_Replica_singleDefaultKeyEncrypted",
            "TestAccDynamoDBTable_Replica_singleDefaultKeyEncryptedAmazonOwned",
            "TestAccDynamoDBTable_Replica_singleStreamSpecification",
            "TestAccDynamoDBTable_Replica_multiple",
            "TestAccDynamoDBTable_Replica_doubleAddCMK",
            "TestAccDynamoDBTable_Replica_pitr",
            "TestAccDynamoDBTable_Replica_pitrKMS",
            "TestAccDynamoDBTable_Replica_tagsUpdate",
            "TestAccDynamoDBTable_Replica_tags_propagateToAddedReplica",
            "TestAccDynamoDBTable_Replica_tags_notPropagatedToAddedReplica",
            "TestAccDynamoDBTable_Replica_tags_nonPropagatedTagsAreUnmanaged",
            "TestAccDynamoDBTable_Replica_tags_updateIsPropagated_oneOfTwo",
            "TestAccDynamoDBTable_Replica_tags_updateIsPropagated_twoOfTwo",
            "TestAccDynamoDBTable_restoreCrossRegion",
            // --- unsupportable: INFREQUENT_ACCESS table class ---
            "TestAccDynamoDBTable_tableClassInfrequentAccess",
            "TestAccDynamoDBTable_tableClass_migrate",
            "TestAccDynamoDBTable_tableClass_ConcurrentModification",
            // --- unsupportable: backup encryption (S3 import/export path) ---
            "TestAccDynamoDBTable_backupEncryption",
            "TestAccDynamoDBTable_backup_overrideEncryption",
            "TestAccDynamoDBTable_importTable",
            // --- gap: DynamoDB Streams shape parity not yet complete ---
            "TestAccDynamoDBTable_streamSpecification",
            "TestAccDynamoDBTable_streamSpecificationDiffs",
            // --- gap: on-demand throughput attribute ---
            "TestAccDynamoDBTable_onDemandThroughput",
            "TestAccDynamoDBTable_gsiOnDemandThroughput",
            // --- gap: billing-mode transitions with GSI ---
            "TestAccDynamoDBTable_BillingMode_payPerRequestBasic",
            "TestAccDynamoDBTable_BillingModeGSI_payPerRequestToProvisioned",
            "TestAccDynamoDBTable_BillingModeGSI_provisionedToPayPerRequest",
            "TestAccDynamoDBTable_Disappears_payPerRequestWithGSI",
            // --- gap: GSI capacity update ---
            "TestAccDynamoDBTable_gsiUpdateCapacity",
            // --- gap: deletion_protection attribute not yet implemented ---
            "TestAccDynamoDBTable_deletion_protection",
            // --- gap: encryption attribute round-trip ---
            "TestAccDynamoDBTable_encryption",
            // --- hung: did not complete in triage run; revisit in a later batch ---
            "TestAccDynamoDBTable_attributeUpdate",
            "TestAccDynamoDBTable_extended",
            "TestAccDynamoDBTable_gsiUpdateNonKeyAttributes",
            "TestAccDynamoDBTable_gsiUpdateOtherAttributes",
            "TestAccDynamoDBTable_TTL_updateDisable",
        ],
    },
];
