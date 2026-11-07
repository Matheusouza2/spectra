package racon;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;

/**
 * Stage-1 acceptance example (spec §57): a standalone Java program that drives
 * the whole RACoN workflow through the JNI bootstrap and the HTTP contract.
 */
public final class RaconExample {
    private RaconExample() {
    }

    public static void main(String[] args) throws Exception {
        String modelPath = args.length > 0 ? args[0] : writeSampleModel();

        RaconServerInfo info = RaconServerInfo.parse(RaconRuntime.startServer());
        System.out.println("[jni]  server " + info.status() + " at " + info.baseUrl()
                + " (pid " + info.pid() + ")");
        try {
            RaconClient client = new RaconClient(info.baseUrl(), info.token());

            System.out.println("[http] health      = " + client.health());
            String sessionId = client.load(modelPath);
            System.out.println("[http] session_id  = " + sessionId);
            System.out.println("[http] ORD         = " + client.property(sessionId, "ORD"));

            String analysisId = client.generateStateSpace(sessionId);
            System.out.println("[http] analysis_id = " + analysisId);

            String status;
            do {
                Thread.sleep(100L);
                status = client.analysisStatus(analysisId);
                System.out.println("[http] progress    = " + status);
            } while ("RUNNING".equals(status) || "CANCEL_REQUESTED".equals(status));

            System.out.println("[http] REV         = " + client.property(sessionId, "REV"));
            System.out.println("[http] invariants  = " + client.invariantCount(sessionId));
        } finally {
            System.out.println("[jni]  stopServer  -> " + RaconRuntime.stopServer());
        }
    }

    private static String writeSampleModel() throws IOException {
        Path path = Files.createTempFile("racon-example", ".mercury");
        Files.writeString(path, """
                SPN Model JavaExample {
                    place Reservoir(tokens = 1);
                    place Processing;
                    timedTransition Start(inputs = [Reservoir], outputs = [Processing], delay = 0.0);
                    timedTransition Finish(inputs = [Processing], outputs = [Reservoir], delay = 0.0);
                }

                main {
                }
                """);
        return path.toString();
    }
}
