package dev.fakecloud;

import java.io.File;
import java.io.IOException;
import java.net.ServerSocket;
import java.net.Socket;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.concurrent.atomic.AtomicReference;

import org.junit.jupiter.api.extension.AfterAllCallback;
import org.junit.jupiter.api.extension.BeforeAllCallback;
import org.junit.jupiter.api.extension.ExtensionContext;

/**
 * JUnit 5 extension that spawns a fresh {@code fakecloud} binary on an ephemeral port
 * for the lifetime of a test class.
 *
 * <p>Mirrors the behavior of {@code sdks/typescript/tests/global-setup.ts}: picks a free
 * port, starts the release (or debug fallback) binary from the workspace {@code target/}
 * directory, waits for TCP readiness, and terminates the process after the test class.
 */
public final class FakeCloudServer implements BeforeAllCallback, AfterAllCallback {
    private static final AtomicReference<String> ENDPOINT = new AtomicReference<>();

    private Process process;

    public static String endpoint() {
        String ep = ENDPOINT.get();
        if (ep == null) {
            throw new IllegalStateException(
                    "FakeCloudServer endpoint not set — is the extension registered?");
        }
        return ep;
    }

    @Override
    public void beforeAll(ExtensionContext context) throws Exception {
        Path repoRoot = locateRepoRoot();
        File releaseBin = repoRoot.resolve("target/release/fakecloud").toFile();
        File debugBin = repoRoot.resolve("target/debug/fakecloud").toFile();

        File bin;
        if (releaseBin.isFile() && releaseBin.canExecute()) {
            bin = releaseBin;
        } else if (debugBin.isFile() && debugBin.canExecute()) {
            bin = debugBin;
        } else {
            throw new IllegalStateException(
                    "fakecloud binary not found. Build it first with: cargo build --release\n"
                            + "  Looked for:\n    "
                            + releaseBin
                            + "\n    "
                            + debugBin);
        }

        int port = freePort();
        String endpoint = "http://127.0.0.1:" + port;

        ProcessBuilder pb = new ProcessBuilder(
                        bin.getAbsolutePath(),
                        "--addr",
                        "127.0.0.1:" + port,
                        "--log-level",
                        "warn")
                .redirectErrorStream(true)
                .redirectOutput(ProcessBuilder.Redirect.DISCARD);
        process = pb.start();

        waitForPort("127.0.0.1", port, 15_000);
        ENDPOINT.set(endpoint);
    }

    @Override
    public void afterAll(ExtensionContext context) {
        ENDPOINT.set(null);
        if (process != null && process.isAlive()) {
            process.destroy();
            try {
                if (!process.waitFor(3, java.util.concurrent.TimeUnit.SECONDS)) {
                    process.destroyForcibly();
                }
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
                process.destroyForcibly();
            }
        }
    }

    private static int freePort() throws IOException {
        try (ServerSocket s = new ServerSocket(0)) {
            return s.getLocalPort();
        }
    }

    private static void waitForPort(String host, int port, long timeoutMs) throws IOException {
        long deadline = System.currentTimeMillis() + timeoutMs;
        while (System.currentTimeMillis() < deadline) {
            try (Socket s = new Socket(host, port)) {
                return;
            } catch (IOException ignored) {
                try {
                    Thread.sleep(100);
                } catch (InterruptedException e) {
                    Thread.currentThread().interrupt();
                    throw new IOException("interrupted while waiting for port", e);
                }
            }
        }
        throw new IOException(
                "fakecloud did not start within " + timeoutMs + "ms on port " + port);
    }

    private static Path locateRepoRoot() {
        // sdks/java/<cwd> -> repo root is 2 levels up
        Path cwd = Paths.get(System.getProperty("user.dir")).toAbsolutePath();
        Path candidate = cwd;
        for (int i = 0; i < 6; i++) {
            if (candidate.resolve("Cargo.toml").toFile().isFile()
                    && candidate.resolve("crates").toFile().isDirectory()) {
                return candidate;
            }
            Path parent = candidate.getParent();
            if (parent == null) break;
            candidate = parent;
        }
        throw new IllegalStateException(
                "could not locate fakecloud repo root from " + cwd);
    }
}
