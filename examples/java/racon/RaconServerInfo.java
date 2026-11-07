package racon;

import java.util.Map;

/** Parsed projection of the JSON returned by {@link RaconRuntime#startServer()}. */
public record RaconServerInfo(
        String status,
        String host,
        int port,
        String protocolVersion,
        String token,
        long pid) {

    public static RaconServerInfo parse(String json) {
        Map<String, Object> value = Json.object(Json.parse(json));
        if (value.containsKey("error")) {
            throw new IllegalStateException("startServer failed: " + json);
        }
        return new RaconServerInfo(
                Json.string(value, "status"),
                Json.string(value, "host"),
                ((Number) value.get("port")).intValue(),
                Json.string(value, "protocol_version"),
                Json.string(value, "token"),
                value.get("pid") == null ? 0L : ((Number) value.get("pid")).longValue());
    }

    public String baseUrl() {
        return "http://" + host + ":" + port;
    }
}
