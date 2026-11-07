package racon;

import java.io.IOException;
import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.util.List;
import java.util.Map;

/** Thin HTTP/JSON client for the local RACoN service. */
public final class RaconClient {
    private final HttpClient http = HttpClient.newHttpClient();
    private final String baseUrl;
    private final String token;

    public RaconClient(String baseUrl, String token) {
        this.baseUrl = baseUrl;
        this.token = token;
    }

    public Map<String, Object> health() {
        return Json.object(Json.parse(request("GET", "/v1/health", null)));
    }

    public String load(String modelPath) {
        Map<String, Object> response =
                Json.object(Json.parse(request("POST", "/v1/nets/load", "{\"path\":\""
                        + escape(modelPath) + "\"}")));
        return Json.string(response, "session_id");
    }

    public Map<String, Object> properties(String sessionId) {
        return Json.object(Json.parse(
                request("GET", "/v1/sessions/" + sessionId + "/properties?scope=all", null)));
    }

    /** Returns the reported status (e.g. {@code TRUE}, {@code UNKNOWN}) for a property code. */
    public String property(String sessionId, String code) {
        List<Object> properties = Json.array(properties(sessionId).get("properties"));
        for (Object candidate : properties) {
            Map<String, Object> property = Json.object(candidate);
            if (code.equals(Json.string(property, "code"))) {
                return Json.string(property, "status");
            }
        }
        return "MISSING";
    }

    public String generateStateSpace(String sessionId) {
        Map<String, Object> response = Json.object(Json.parse(
                request("POST", "/v1/sessions/" + sessionId + "/state-space", "{}")));
        return Json.string(response, "analysis_id");
    }

    public String analysisStatus(String analysisId) {
        Map<String, Object> response = Json.object(Json.parse(
                request("GET", "/v1/analyses/" + analysisId + "/status", null)));
        return Json.string(response, "status");
    }

    public int invariantCount(String sessionId) {
        Map<String, Object> response = Json.object(Json.parse(
                request("GET", "/v1/sessions/" + sessionId + "/invariants?kind=ALL", null)));
        return Json.array(response.get("invariants")).size();
    }

    private String request(String method, String path, String body) {
        try {
            HttpRequest.Builder builder = HttpRequest.newBuilder(URI.create(baseUrl + path))
                    .header("Accept", "application/json");
            if (token != null && !token.isEmpty()) {
                builder.header("Authorization", "Bearer " + token);
            }
            if (body == null) {
                builder.method(method, HttpRequest.BodyPublishers.noBody());
            } else {
                builder.header("Content-Type", "application/json");
                builder.method(method, HttpRequest.BodyPublishers.ofString(body));
            }
            HttpResponse<String> response =
                    http.send(builder.build(), HttpResponse.BodyHandlers.ofString());
            if (response.statusCode() >= 400) {
                throw new IllegalStateException("RACoN " + method + " " + path + " failed with HTTP "
                        + response.statusCode() + ": " + response.body());
            }
            return response.body();
        } catch (IOException exception) {
            throw new IllegalStateException("RACoN request failed: " + exception.getMessage(), exception);
        } catch (InterruptedException exception) {
            Thread.currentThread().interrupt();
            throw new IllegalStateException("RACoN request interrupted", exception);
        }
    }

    private static String escape(String value) {
        return value.replace("\\", "\\\\").replace("\"", "\\\"");
    }
}
