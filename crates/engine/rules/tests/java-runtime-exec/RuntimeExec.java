import java.io.IOException;

public class RuntimeExec {
    public static void run(String cmd) throws IOException {
        // ruleid: java/runtime-exec
        Process p = Runtime.getRuntime().exec(cmd);
        p.waitFor();
    }

    public static long total() {
        return Runtime.getRuntime().totalMemory();
    }
}
