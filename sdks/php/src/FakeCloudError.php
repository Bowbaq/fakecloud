<?php

declare(strict_types=1);

namespace FakeCloud;

use RuntimeException;

final class FakeCloudError extends RuntimeException
{
    public function __construct(
        public readonly int $status,
        public readonly string $body,
    ) {
        parent::__construct("fakecloud API error ({$status}): {$body}");
    }
}
