package dev.fakecloud;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

class ClientUnitTest {

    @Test
    void trimsTrailingSlashesFromBaseUrl() {
        assertEquals("http://localhost:4566", FakeCloud.trimTrailingSlashes("http://localhost:4566"));
        assertEquals("http://localhost:4566", FakeCloud.trimTrailingSlashes("http://localhost:4566/"));
        assertEquals("http://localhost:4566", FakeCloud.trimTrailingSlashes("http://localhost:4566///"));
    }

    @Test
    void defaultBaseUrlMatchesSiblingSdks() {
        FakeCloud fc = new FakeCloud();
        assertEquals("http://localhost:4566", fc.baseUrl());
    }

    @Test
    void encodePathTurnsSpacesIntoPercent20() {
        assertEquals("hello%20world", HttpTransport.encodePath("hello world"));
        assertEquals("a%2Fb", HttpTransport.encodePath("a/b"));
        assertEquals("plain", HttpTransport.encodePath("plain"));
    }

    @Test
    void errorCarriesStatusAndBody() {
        FakeCloudError err = new FakeCloudError(503, "upstream unavailable");
        assertEquals(503, err.status());
        assertEquals("upstream unavailable", err.body());
        assertTrue(err.getMessage().contains("503"));
        assertTrue(err.getMessage().contains("upstream unavailable"));
    }

    @Test
    void networkFailureIsSurfacedAsFakeCloudError() {
        FakeCloud fc = new FakeCloud("http://127.0.0.1:1");
        FakeCloudError err = assertThrows(FakeCloudError.class, fc::health);
        assertEquals(-1, err.status());
    }
}
