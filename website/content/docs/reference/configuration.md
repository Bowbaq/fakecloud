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
