+++
title = "DynamoDB"
description = "Tables, items, transactions, PartiQL, backups, global tables, streams, TTL."
weight = 6
+++

fakecloud implements **57 of 57** DynamoDB operations at 100% Smithy conformance.

## Supported features

- **Tables** — CRUD, attributes, indexes (GSI, LSI), billing modes, tags
- **Items** — GetItem, PutItem, UpdateItem, DeleteItem, BatchGetItem, BatchWriteItem
- **Transactions** — TransactGetItems, TransactWriteItems with conditional checks
- **Query and Scan** — full expression support (key conditions, filter expressions)
- **PartiQL** — ExecuteStatement, BatchExecuteStatement, ExecuteTransaction
- **Update expressions** — SET, REMOVE, ADD, DELETE with function support (`size`, `attribute_exists`, `begins_with`, `contains`, `attribute_type`)
- **Condition expressions** — full operator support with correct type coercion
- **Global tables** — replica management, replica status reporting
- **Backups** — CreateBackup, DescribeBackup, RestoreTableFromBackup
- **Streams** — shard iterators, record retrieval, delivery to Lambda/Kinesis
- **TTL** — expire items via `/_fakecloud/dynamodb/ttl-processor/tick`
- **Exports and imports** — S3 exports (recorded), S3 imports (recorded)

## Protocol

JSON protocol. `X-Amz-Target` header, JSON body, JSON responses.

## Introspection

- `POST /_fakecloud/dynamodb/ttl-processor/tick` — expire items whose TTL attribute is in the past

## Cross-service delivery

- **DynamoDB Streams -> Lambda** — Event source mapping polls and invokes
- **DynamoDB -> Kinesis** — Table changes stream to Kinesis Data Streams

## Source

- [`crates/fakecloud-dynamodb`](https://github.com/faiscadev/fakecloud/tree/main/crates/fakecloud-dynamodb)
- [AWS DynamoDB API reference](https://docs.aws.amazon.com/amazondynamodb/latest/APIReference/Welcome.html)
