package racon;

/**
 * Stage-1 acceptance example (spec §57): bootstrap via JNI, then perform every
 * analysis strictly through HTTP/JSON.
 */
public final class RaconHttpExample {
    private RaconHttpExample() {}

    public static void main(String[] args) throws Exception {
        RaconExample.main(args);
    }
}
