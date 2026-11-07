package racon;

/**
 * Java-facing bootstrap for the local RACoN service.
 *
 * <p>This class only starts and stops the process. All analysis is performed
 * over HTTP/JSON; Java never sees Rust pointers, internal structures or Rayon
 * internals.
 */
public final class RaconRuntime {
    static {
        System.loadLibrary("racon_jni");
    }

    private RaconRuntime() {
    }

    /** Starts the service (idempotent) and returns a JSON {@code ServerInfo}. */
    public static native String startServer();

    /** Stops the service (idempotent) and returns its terminal JSON state. */
    public static native String stopServer();

    /** Returns the current lifecycle state as JSON. */
    public static native String serverStatus();
}
