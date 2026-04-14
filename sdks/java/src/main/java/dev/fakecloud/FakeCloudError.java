package dev.fakecloud;

/** Thrown when a fakecloud introspection endpoint returns a non-2xx response. */
public class FakeCloudError extends RuntimeException {
    private final int status;
    private final String body;

    public FakeCloudError(int status, String body) {
        super("fakecloud API error (" + status + "): " + body);
        this.status = status;
        this.body = body;
    }

    public int status() {
        return status;
    }

    public String body() {
        return body;
    }
}
