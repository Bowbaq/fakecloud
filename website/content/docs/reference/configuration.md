+++
title = "Configuration"
description = "CLI flags and environment variables for fakecloud."
weight = 1
+++

fakecloud is configured via CLI flags or environment variables. Flags take precedence when both are set.

| Flag                 | Env Var                     | Default            | Description                                                                              |
| -------------------- | --------------------------- | ------------------ | ---------------------------------------------------------------------------------------- |
| `--addr`             | `FAKECLOUD_ADDR`            | `0.0.0.0:4566`     | Listen address and port                                                                  |
| `--region`           | `FAKECLOUD_REGION`          | `us-east-1`        | AWS region to advertise                                                                  |
| `--account-id`       | `FAKECLOUD_ACCOUNT_ID`      | `123456789012`     | AWS account ID                                                                           |
| `--log-level`        | `FAKECLOUD_LOG`             | `info`             | Log level (trace, debug, info, warn, error)                                              |
| `--storage-mode`     | `FAKECLOUD_STORAGE_MODE`    | `memory`           | `memory` (default, all state in RAM) or `persistent` (mirror state to `--data-path`)    |
| `--data-path`        | `FAKECLOUD_DATA_PATH`       | —                  | Directory to persist state to. Required when `--storage-mode=persistent`.                |
| `--s3-cache-size`    | `FAKECLOUD_S3_CACHE_SIZE`   | `268435456`        | In-memory LRU cache for S3 object bodies in persistent mode. Default 256 MiB.            |
|                      | `FAKECLOUD_CONTAINER_CLI`   | auto-detect        | Container CLI to use (`docker` or `podman`)                                              |

## Examples

```sh
# Bind to localhost only
fakecloud --addr 127.0.0.1:4566

# Verbose logging
fakecloud --log-level debug

# Different region and account
fakecloud --region eu-west-1 --account-id 999999999999

# Persistent storage
fakecloud --storage-mode persistent --data-path /var/lib/fakecloud
```

## Environment-only configuration

```sh
FAKECLOUD_LOG=trace fakecloud
FAKECLOUD_REGION=eu-central-1 fakecloud
```

See also [Persistence](/docs/reference/persistence/) for details on persistent storage mode.

## LocalStack URL compatibility

fakecloud decodes LocalStack's `*.localhost.localstack.cloud` hostname convention so persisted URLs from a LocalStack setup — queue URLs baked into dev scripts, presigned URLs in fixtures, webhook targets in response mocks — replay against fakecloud without rewriting. Two patterns are recognized on the `Host` header:

| Host pattern                                                | Routed as                             |
| ----------------------------------------------------------- | ------------------------------------- |
| `<service>.<region>.localhost.localstack.cloud[:port]`      | `<service>` in `<region>`             |
| `<bucket>.s3.<region>.localhost.localstack.cloud[:port]`    | S3 virtual-hosted-style on `<bucket>` |

The DNS wildcard `*.localhost.localstack.cloud` resolves to `127.0.0.1`, so TCP reaches fakecloud unchanged; fakecloud then parses the hostname to recover service, region, and (for S3) bucket. SigV4-signed requests still route by credential scope first — the hostname is a secondary signal that takes over when the request is unsigned, uses a non-standard `Authorization` header, or is being probed with `curl`.

```bash
# Unsigned SQS request — routed to SQS purely by Host header
curl -X POST \
     -H 'Host: sqs.us-east-1.localhost.localstack.cloud:4566' \
     -d 'Action=ListQueues&Version=2012-11-05' \
     http://127.0.0.1:4566/

# Virtual-hosted-style S3 GetObject — bucket recovered from Host header
curl -H 'Host: my-bucket.s3.us-east-1.localhost.localstack.cloud:4566' \
     http://127.0.0.1:4566/key
```

Bucket names with dots (e.g. `a.b.c`) are supported; fakecloud recognizes the `.s3.<region>.localhost.localstack.cloud` suffix and treats everything before it as the bucket label.
