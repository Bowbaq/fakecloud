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
        name: "sqs",
        // Batch 2: enable the full upstream SQS suite. Triage with the
        // redrive_policy whitespace fix shipped in this batch leaves four
        // encryption tests failing (default `kms_data_key_reuse_period_seconds`,
        // managed-SSE state transitions). Batch 3 closes those.
        run_regex: "^TestAcc",
        deny: &[
            // --- gap: KmsDataKeyReusePeriodSeconds default and managed-SSE
            //          mode-switch reset; closed in Batch 3 ---
            "TestAccSQSQueue_encryption",
            "TestAccSQSQueue_managedEncryption",
            "TestAccSQSQueue_defaultKMSDataKeyReusePeriodSeconds",
            "TestAccSQSQueue_ManagedEncryption_kmsDataKeyReusePeriodSeconds",
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
