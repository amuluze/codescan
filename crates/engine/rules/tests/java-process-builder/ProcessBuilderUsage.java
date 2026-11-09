import java.io.IOException;
import java.util.List;

public class ProcessBuilderUsage {
    public static void run(List<String> cmd) throws IOException {
        // ruleid: java/process-builder
        Process p = new ProcessBuilder(cmd).start();
        p.waitFor();
    }
}
